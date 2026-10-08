use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::oneshot;

use crate::pty::{
    enqueue_message, now_ms, record_msg, withdraw_message, write_to_pty, AskWaiter, LedgerEntry,
    PtyStateInner, QueuedMsg,
};
use crate::browser::BrowserState;
use crate::chat::ChatState;
use crate::routines::RoutinesState;
use crate::screen::TurnState;

/// `ask` blocks until the target finishes — the caller's own tool timeout is
/// the real bound; this only reclaims waiters nobody is reading anymore.
const ASK_DEFAULT_TIMEOUT: Duration = Duration::from_secs(3600);
const CANVAS_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(250);
/// After delivery, the target must stay settled this long before the answer
/// is read (the screen-state settle window already applies on top).
const MIN_TURN: Duration = Duration::from_millis(800);
/// How long an ask waits for a remote teammate that is still starting.
const BOOT_WAIT: Duration = Duration::from_secs(120);
/// `--raw` returns once the terminal reacts and settles, at most this long.
const RAW_MAX_WAIT: Duration = Duration::from_secs(10);
/// Ledger copies of replies are capped — the full text goes to the caller.
const LEDGER_REPLY_CAP: usize = 4000;

/// Skills installed for the agents: (directory name, SKILL.md content).
const SKILLS: &[(&str, &str)] = &[
    ("narrater", include_str!("../assets/SKILL.md")),
    ("narrater-manager", include_str!("../assets/SKILL-manager.md")),
    ("narrater-portal", include_str!("../assets/SKILL-portal.md")),
    ("narrater-portal-devices", include_str!("../assets/SKILL-portal-devices.md")),
    ("narrater-routines", include_str!("../assets/SKILL-routines.md")),
    ("narrater-workspace", include_str!("../assets/SKILL-workspace.md")),
    ("narrater-chat", include_str!("../assets/SKILL-chat.md")),
];
/// The skills, as (name, SKILL.md) — also installed in remote workspaces.
pub fn skills() -> Vec<(&'static str, &'static str)> {
    SKILLS.to_vec()
}

/// Skill directories read by the agent CLIs: Claude Code, and the open Agent
/// Skills location shared by Codex, Gemini CLI, OpenCode and others.
const SKILL_BASES: &[&str] = &[".claude/skills", ".agents/skills"];

#[derive(Deserialize)]
struct IpcRequest {
    /// Caller node id (NARRATER_ID inside the terminal).
    #[serde(default)]
    from: String,
    /// Terminal secret (env NARRATER_TOKEN) proving the `from` identity.
    #[serde(default)]
    token: Option<String>,
    /// "ping" | "whoami" | "ask" | "batch" | "raw" | "check" | "canvas" | "portal" | "routine" | "workspace" | "chat"
    #[serde(default)]
    mode: Option<String>,
    /// Target agent name (label) or node id.
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    /// mode "batch": agent name → prompt.
    #[serde(default)]
    batch: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    /// mode "canvas": action handled by the frontend (list, note_*, connect);
    /// mode "routine": the parsed flags.
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    params: Option<serde_json::Value>,
    /// mode "portal": the verb's arguments after the portal name.
    #[serde(default)]
    args: Option<Vec<String>>,
}

/// Per-process socket inside a private (0700) directory.
pub fn socket_path() -> String {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("narrater-{}", std::process::id()));
        dir.join("narrater.sock").display().to_string()
    })
    .clone()
}

pub async fn start_ipc_server(
    app: AppHandle,
    state: Arc<Mutex<PtyStateInner>>,
    browser: BrowserState,
    routines: RoutinesState,
    chat: ChatState,
) {
    install_cli();
    install_skills();
    remove_legacy_files();
    remove_stale_socket_dirs();

    let socket_path = socket_path();
    let dir = PathBuf::from(&socket_path).parent().map(PathBuf::from).unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[NarraTer IPC] Failed to bind socket {}: {}", socket_path, e);
            return;
        }
    };
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));

    let ctx = IpcCtx { app, state, browser, routines, chat };
    // Remote workspaces (SSH, Docker) reach the same dispatch over HTTP
    tokio::spawn(crate::bridge::serve(ctx.clone()));

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_connection(stream, ctx.clone()));
            }
            Err(e) => eprintln!("[NarraTer IPC] Accept error: {}", e),
        }
    }
}

/// Everything a CLI request may need.
#[derive(Clone)]
pub struct IpcCtx {
    pub app: AppHandle,
    pub state: Arc<Mutex<PtyStateInner>>,
    pub browser: BrowserState,
    pub routines: RoutinesState,
    pub chat: ChatState,
}

async fn handle_connection(mut stream: tokio::net::UnixStream, ctx: IpcCtx) {
    let mut buf = Vec::new();
    if stream.read_to_end(&mut buf).await.is_err() {
        return;
    }
    let reply = dispatch(&ctx, &buf).await;
    let _ = stream.write_all(reply.as_bytes()).await;
}

/// Runs one CLI request (the JSON `cli_core` builds); returns what to print.
pub async fn dispatch(ctx: &IpcCtx, buf: &[u8]) -> String {
    let IpcCtx { app, state, browser, routines, chat } = ctx;
    let req: IpcRequest = match serde_json::from_slice(buf) {
        Ok(r) => r,
        Err(_) => return "Error: invalid request format\n".to_string(),
    };

    match authenticate(&req, state) {
        Err(e) => e,
        Ok(()) => match req.mode.as_deref() {
            Some("ping") => "ok\n".to_string(),
            Some("whoami") => {
                let inner = state.lock().unwrap();
                format!("id: {}\nname: {}\n", req.from, inner.label_of(&req.from))
            }
            Some("ask") => {
                let timeout = req.timeout_secs.map(Duration::from_secs).unwrap_or(ASK_DEFAULT_TIMEOUT);
                match ask_one(state, app, &req.from, req.to.as_deref().unwrap_or(""), req.msg.as_deref().unwrap_or(""), timeout).await {
                    Ok(text) => with_newline(text),
                    Err(e) => with_newline(e),
                }
            }
            Some("batch") => handle_batch(&req, app, state).await,
            Some("raw") => handle_raw(&req, app, state).await,
            Some("check") => handle_check(&req, state),
            Some("canvas") => handle_canvas(&req, app, state).await,
            Some("chat") => {
                let params = req.params.clone().unwrap_or(serde_json::Value::Null);
                with_newline(crate::chat::run_cli(chat, app, &req.from, &params))
            }
            Some("workspace") => {
                let params = req.params.clone().unwrap_or(serde_json::Value::Null);
                // "yours" in `workspace list` is the caller's own scene
                let mine = Some(state.lock().unwrap().names.scene_of(&req.from)).filter(|s| !s.is_empty());
                with_newline(crate::workspaces::run_cli(app, state, &req.from, mine, &params))
            }
            Some("routine") => {
                let params = req.params.clone().unwrap_or(serde_json::Value::Null);
                with_newline(routines.run_cli(app, state, &req.from, &params).await)
            }
            Some("portal") => {
                let verb = req.msg.as_deref().unwrap_or("");
                let args = req.args.clone().unwrap_or_default();
                with_newline(browser.run_cli(app, &req.from, req.to.as_deref().unwrap_or(""), verb, &args).await)
            }
            _ => "Error: unknown command — run `narrater help`\n".to_string(),
        },
    }
}

fn with_newline(s: String) -> String {
    if s.ends_with('\n') { s } else { s + "\n" }
}

/// Anti-spoof: the caller proves its `from` identity with its own terminal's
/// token (env NARRATER_TOKEN).
fn authenticate(req: &IpcRequest, state: &Arc<Mutex<PtyStateInner>>) -> Result<(), String> {
    if req.mode.as_deref() == Some("ping") {
        return Ok(());
    }
    if req.from.is_empty() {
        return Err("Error: NARRATER_ID not set — run this from a NarraTer terminal (`narrater debug` to diagnose)\n".into());
    }
    let inner = state.lock().unwrap();
    match inner.sessions.get(&req.from) {
        None => Err("Error: unknown terminal — restart it from NarraTer (`narrater debug` to diagnose)\n".into()),
        Some(s) if req.token.as_deref() != Some(s.token.as_str()) => {
            Err("Error: invalid NARRATER_TOKEN — the given identity doesn't match\n".into())
        }
        Some(_) => Ok(()),
    }
}

/// Resolves `to` (exact name, node id, then case-insensitive name) among the
/// agents connected to `from`. Returns (target id, caller name, target name).
fn resolve_target(inner: &PtyStateInner, from: &str, to: &str) -> Result<(String, String, String), String> {
    let to = to.trim();
    if to.is_empty() {
        return Err("Error: no agent name given — run `narrater list` for the exact names".into());
    }
    let target_id = inner
        .names
        .resolve(from, to, |id| inner.connected(from, id))
        .or_else(|| inner.sessions.contains_key(to).then(|| to.to_string()))
        .ok_or_else(|| format!("Error: no agent named '{}' — run `narrater list` for the exact names", to))?;
    if target_id == from {
        return Err("Error: that's you — pick a connected agent from `narrater list`".into());
    }
    if !inner.connected(from, &target_id) {
        return Err(format!(
            "Error: '{}' is not connected to you — only agents connected on the canvas can be reached (see `narrater list`)",
            inner.address(&target_id, from)
        ));
    }
    if !inner.sessions.contains_key(&target_id) {
        if inner.booting.contains(&target_id) {
            return Err(format!(
                "Error: '{}' is still starting (connecting to its workspace) — try again in a moment",
                inner.address(&target_id, from)
            ));
        }
        return Err(format!("Error: '{}' is not running — ask the user to restart it on the canvas", to));
    }
    // Each side sees the other the way it would address it back
    Ok((target_id.clone(), inner.address(from, &target_id), inner.address(&target_id, from)))
}

fn notice_ask_back(from_label: &str) -> String {
    format!(
        "'{}' responded by messaging you directly instead of finishing quietly. Their message arrives in your terminal as a new incoming prompt; treat it as the reply to this ask.",
        from_label
    )
}

fn notice_not_finished(to_label: &str) -> String {
    format!(
        "'{0}' has NOT finished yet: it received your prompt and keeps working; it will message you or you can run `narrater check \"{0}\"` to follow it. Do NOT re-send the prompt.",
        to_label
    )
}

fn notice_attention(to_label: &str, screen: &str) -> String {
    format!(
        "'{0}' stopped and is waiting for input (permission prompt or menu). Answer with `narrater ask \"{0}\" --raw \"...\"` or tell the user. Current screen:\n\n{1}",
        to_label, screen
    )
}

/// The heart of the protocol, agnostic to which AI runs in the target:
/// type the prompt into its terminal when it's idle, wait until its screen
/// says the turn is over, and return what it shows. If the target messages the
/// caller back meanwhile (ask-back), that resolves this ask instead.
pub async fn ask_one(
    state: &Arc<Mutex<PtyStateInner>>,
    app: &AppHandle,
    from: &str,
    to: &str,
    msg: &str,
    timeout: Duration,
) -> Result<String, String> {
    if msg.trim().is_empty() {
        return Err("Error: empty prompt".into());
    }
    // A remote teammate still connecting (setup over SSH / docker exec) is
    // waited for, like a local one booting
    let boot_deadline = tokio::time::Instant::now() + BOOT_WAIT;
    let (target_id, from_label, to_label) = loop {
        let starting = {
            let inner = state.lock().unwrap();
            match resolve_target(&inner, from, to) {
                Ok(r) => break r,
                Err(e) => {
                    let id = inner.names.resolve(from, to, |id| inner.connected(from, id));
                    if id.is_some_and(|id| inner.booting.contains(&id)) { e } else { return Err(e) }
                }
            }
        };
        if tokio::time::Instant::now() >= boot_deadline {
            return Err(starting);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    };

    // Ask-back: the target of this ask is blocked waiting on *us* — this
    // message is its answer. Wake it now; the message itself still arrives
    // in its terminal in full as a new prompt.
    let backs: Vec<oneshot::Sender<String>> = {
        let mut inner = state.lock().unwrap();
        let keys: Vec<String> = inner
            .ask_waiters
            .iter()
            .filter(|(_, w)| w.asker_id == target_id && w.target_id == from)
            .map(|(k, _)| k.clone())
            .collect();
        keys.iter().filter_map(|k| inner.ask_waiters.remove(k)).map(|w| w.tx).collect()
    };
    for tx in backs {
        let _ = tx.send(notice_ask_back(&from_label));
    }

    let ask_id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = oneshot::channel::<String>();
    let (delivered_tx, delivered_rx) = oneshot::channel::<()>();
    state.lock().unwrap().ask_waiters.insert(ask_id.clone(), AskWaiter {
        asker_id: from.to_string(),
        target_id: target_id.clone(),
        tx,
    });
    enqueue_message(state, app, &target_id, QueuedMsg {
        from_label: from_label.clone(),
        from_id: Some(from.to_string()),
        msg: msg.to_string(),
        delivered_tx: Some(delivered_tx),
    });
    record_msg(state, app, LedgerEntry {
        from: from.to_string(),
        to: target_id.clone(),
        from_label: from_label.clone(),
        to_label: to_label.clone(),
        kind: "ask".into(),
        msg: msg.to_string(),
        ts: now_ms(),
    });

    let deadline = Instant::now() + timeout;
    let cleanup = |state: &Arc<Mutex<PtyStateInner>>| {
        state.lock().unwrap().ask_waiters.remove(&ask_id);
    };

    // Phase 1: the prompt waits in the queue until the target is idle.
    tokio::select! {
        r = delivered_rx => {
            if r.is_err() {
                cleanup(state);
                return Err(format!("Error: the prompt to '{}' was cancelled before delivery", to_label));
            }
        }
        back = &mut rx => {
            // Answered by ask-back before our prompt even went in — it stays
            // queued and will still be delivered.
            return back.map_err(|_| format!("Error: '{}' exited", to_label));
        }
        _ = tokio::time::sleep_until(deadline.into()) => {
            cleanup(state);
            let me = from.to_string();
            let m = msg.to_string();
            withdraw_message(state, app, &target_id, move |q| q.from_id.as_deref() == Some(me.as_str()) && q.msg == m);
            return Err(format!("Error: '{}' stayed busy and never took the prompt; it was withdrawn from its queue — try again later", to_label));
        }
    }
    let delivered_at = Instant::now();

    // Phase 2: wait for the turn to end, read from the screen.
    let result = loop {
        tokio::select! {
            back = &mut rx => {
                break back.map_err(|_| format!("Error: '{}' exited before finishing", to_label));
            }
            _ = tokio::time::sleep(POLL) => {
                let snapshot = {
                    let inner = state.lock().unwrap();
                    inner.sessions.get(&target_id).map(|s| (s.turn, s.screen.text()))
                };
                let Some((turn, screen)) = snapshot else {
                    break Err(format!("Error: '{}' exited before finishing", to_label));
                };
                if delivered_at.elapsed() >= MIN_TURN {
                    match turn {
                        TurnState::Idle => break Ok(screen),
                        TurnState::Attention => break Ok(notice_attention(&to_label, &screen)),
                        TurnState::Working => {}
                    }
                }
                if Instant::now() >= deadline {
                    break Err(notice_not_finished(&to_label));
                }
            }
        }
    };
    cleanup(state);

    if let Ok(text) = &result {
        let mut shown: String = text.chars().take(LEDGER_REPLY_CAP).collect();
        if shown.len() < text.len() {
            shown.push('…');
        }
        record_msg(state, app, LedgerEntry {
            from: target_id.clone(),
            to: from.to_string(),
            from_label: to_label,
            to_label: from_label,
            kind: "reply".into(),
            msg: shown,
            ts: now_ms(),
        });
    }
    result
}

/// `ask --batch`: every target in parallel; JSON array once all are done.
async fn handle_batch(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let Some(batch) = req.batch.clone().filter(|b| !b.is_empty()) else {
        return "Error: --batch takes a JSON object of agent name → prompt\n".into();
    };
    let timeout = req.timeout_secs.map(Duration::from_secs).unwrap_or(ASK_DEFAULT_TIMEOUT);
    let mut set = tokio::task::JoinSet::new();
    for (i, (name, prompt)) in batch.into_iter().enumerate() {
        let (state, app, from) = (Arc::clone(state), app.clone(), req.from.clone());
        set.spawn(async move {
            let res = match prompt.as_str() {
                Some(p) => ask_one(&state, &app, &from, &name, p, timeout).await,
                None => Err("Error: the prompt must be a JSON string".into()),
            };
            (i, name, res)
        });
    }
    let mut out = Vec::new();
    while let Some(Ok(r)) = set.join_next().await {
        out.push(r);
    }
    out.sort_by_key(|(i, _, _)| *i);
    let arr: Vec<serde_json::Value> = out
        .into_iter()
        .map(|(_, name, res)| match res {
            Ok(output) => serde_json::json!({ "name": name, "output": output }),
            Err(error) => serde_json::json!({ "name": name, "error": error.trim_start_matches("Error: ") }),
        })
        .collect();
    with_newline(serde_json::to_string_pretty(&arr).unwrap_or_default())
}

/// `ask --raw`: bytes straight into the target's terminal (menus, keys,
/// shell input); returns the resulting screen once it settles.
async fn handle_raw(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let data = req.msg.clone().unwrap_or_default();
    if data.is_empty() {
        return "Error: empty input\n".into();
    }
    let (target_id, from_label, to_label) = {
        let inner = state.lock().unwrap();
        match resolve_target(&inner, &req.from, req.to.as_deref().unwrap_or("")) {
            Ok(r) => r,
            Err(e) => return with_newline(e),
        }
    };
    write_to_pty(state, &target_id, &data);
    record_msg(state, app, LedgerEntry {
        from: req.from.clone(),
        to: target_id.clone(),
        from_label,
        to_label: to_label.clone(),
        kind: "raw".into(),
        msg: data.escape_debug().to_string(),
        ts: now_ms(),
    });
    let started = Instant::now();
    loop {
        tokio::time::sleep(POLL).await;
        let snapshot = {
            let inner = state.lock().unwrap();
            inner.sessions.get(&target_id).map(|s| (s.turn, s.screen.text()))
        };
        let Some((turn, screen)) = snapshot else {
            return format!("Error: '{}' exited\n", to_label);
        };
        if (started.elapsed() >= MIN_TURN && turn != TurnState::Working) || started.elapsed() >= RAW_MAX_WAIT {
            return with_newline(screen);
        }
    }
}

/// `check`: what the agent's terminal shows right now, nothing sent.
fn handle_check(req: &IpcRequest, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let inner = state.lock().unwrap();
    let (target_id, _, _) = match resolve_target(&inner, &req.from, req.to.as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(e) => return with_newline(e),
    };
    let Some(s) = inner.sessions.get(&target_id) else { return "Error: not running\n".into() };
    let text = s.screen.text();
    let status = match s.turn {
        TurnState::Working => "working",
        TurnState::Idle => "idle",
        TurnState::Attention => "waiting for input",
    };
    format!("[{} · {}]\n{}\n", inner.label_of(&target_id), status, if text.is_empty() { "(empty screen)" } else { &text })
}

#[derive(Clone, serde::Serialize)]
struct CanvasRequestEvent {
    req_id: String,
    from: String,
    from_label: String,
    action: String,
    params: serde_json::Value,
}

/// CLI → canvas bridge (list, notes, connect): the frontend owns the canvas
/// graph, applies the action on its store and answers via canvas_respond.
async fn handle_canvas(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let action = match req.action.as_deref() {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => return "Error: no canvas action given\n".to_string(),
    };

    let req_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<String>();
    let from_label = {
        let mut inner = state.lock().unwrap();
        inner.canvas_waiters.insert(req_id.clone(), tx);
        inner.label_of(&req.from)
    };

    let _ = app.emit("canvas_request", CanvasRequestEvent {
        req_id: req_id.clone(),
        from: req.from.clone(),
        from_label,
        action,
        params: req.params.clone().unwrap_or(serde_json::Value::Null),
    });

    match tokio::time::timeout(CANVAS_TIMEOUT, rx).await {
        Ok(Ok(result)) => with_newline(result),
        _ => {
            state.lock().unwrap().canvas_waiters.remove(&req_id);
            "Error: timeout waiting for the canvas\n".to_string()
        }
    }
}

fn write_file(path: &std::path::Path, content: &str, mode: u32) {
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("[NarraTer] Could not create {}: {}", dir.display(), e);
            return;
        }
    }
    if let Err(e) = std::fs::write(path, content) {
        eprintln!("[NarraTer] Failed to write {}: {}", path.display(), e);
        return;
    }
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

/// The `narrater` CLI every agent reaches through its shell tool: the
/// `narrater-cli` binary shipped next to the app, copied (not linked — an
/// AppImage's mount goes away) to ~/.local/bin/narrater. The copy goes
/// through a temp file + rename, so an agent running the old one mid-call
/// isn't disturbed ("text file busy").
fn install_cli() {
    let Some(home) = dirs::home_dir() else { return };
    let Some(src) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("narrater-cli"))) else { return };
    let dest = home.join(".local/bin/narrater");
    if let Err(e) = install_binary(&src, &dest) {
        eprintln!("[NarraTer] Could not install the CLI from {} to {}: {}", src.display(), dest.display(), e);
    }
}

fn install_binary(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    let bytes = std::fs::read(src)?;
    if std::fs::read(dest).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let dir = dest.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".narrater-cli.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e
    })
}

/// Teaches the protocol to any agent CLI that supports skills — no vendor
/// flags, no system prompt injection.
fn install_skills() {
    let Some(home) = dirs::home_dir() else { return };
    for base in SKILL_BASES {
        for (name, content) in SKILLS {
            write_file(&home.join(base).join(name).join("SKILL.md"), content, 0o644);
        }
    }
}

/// Socket directories of NarraTer processes that are gone (crash, kill -9).
fn remove_stale_socket_dirs() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix("narrater-")) else { continue };
        if pid.parse::<u32>().is_ok() && !std::path::Path::new("/proc").join(pid).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The MCP server and the Claude Stop hook settings are gone: the CLI +
/// skill + screen-state protocol replaced them. Remove our stale copies.
fn remove_legacy_files() {
    let Some(home) = dirs::home_dir() else { return };
    let mcp = home.join(".local/bin/narrater-mcp");
    if std::fs::read_to_string(&mcp).is_ok_and(|s| s.contains("narrater-mcp - MCP server")) {
        let _ = std::fs::remove_file(&mcp);
    }
    let _ = std::fs::remove_file(home.join(".local/share/narrater/claude-hooks.json"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::os::unix::fs::MetadataExt;

    fn inner_with(labels: &[(&str, &str)], connections: &[(&str, &str)]) -> PtyStateInner {
        let mut inner = PtyStateInner {
            connections: connections.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<HashSet<_>>(),
            ..Default::default()
        };
        for (id, label) in labels {
            // "Name@Scene" puts a terminal in another scene (default: "S")
            let (label, scene) = label.split_once('@').unwrap_or((label, "S"));
            inner.names.set(id, scene, label);
        }
        inner
    }

    #[test]
    fn resolve_target_rejects_unknown_self_and_unconnected() {
        let inner = inner_with(&[("t1", "Lead"), ("t2", "Dev"), ("t3", "Ops")], &[("t2", "t1")]);
        assert!(resolve_target(&inner, "t1", "Nobody").unwrap_err().contains("no agent named"));
        assert!(resolve_target(&inner, "t1", "Lead").unwrap_err().contains("that's you"));
        assert!(resolve_target(&inner, "t1", "Ops").unwrap_err().contains("not connected"));
        // Connected (in either direction) but no live session
        assert!(resolve_target(&inner, "t1", "dev").unwrap_err().contains("not running"));
    }

    #[test]
    fn resolve_target_across_workspaces() {
        let mut inner = inner_with(&[("t1", "Lead"), ("t2", "Dev@Front"), ("t3", "Dev")], &[]);
        inner.links.add(
            crate::names::LinkEnd { id: "t1".into(), scene: "S".into(), label: "Lead".into() },
            crate::names::LinkEnd { id: "t2".into(), scene: "Front".into(), label: "Dev".into() },
        );
        // Bare "Dev" is this scene's Dev (not connected); the linked one is "Dev @ Front"
        assert!(resolve_target(&inner, "t1", "Dev").unwrap_err().contains("not connected"));
        let err = resolve_target(&inner, "t1", "Dev @ Front").unwrap_err();
        assert!(err.contains("not running"), "{}", err);
        // The linked end sees the caller across scenes, and vice versa
        assert_eq!(inner.address("t1", "t2"), "Lead @ S");
        assert_eq!(inner.address("t2", "t1"), "Dev @ Front");
        // From Front, its recruiter is reachable by bare name too
        assert!(resolve_target(&inner, "t2", "Lead").unwrap_err().contains("not running"));
    }

    #[test]
    fn cli_install_replaces_atomically_and_skips_identical() {
        let dir = std::env::temp_dir().join(format!("narrater-cli-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dest) = (dir.join("narrater-cli"), dir.join("bin/narrater"));
        std::fs::write(&src, b"v1").unwrap();
        install_binary(&src, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"v1");
        assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o755);
        // An old script in place (the Python CLI) is replaced
        std::fs::write(&dest, b"#!/usr/bin/env python3").unwrap();
        install_binary(&src, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"v1");
        // Identical: untouched (same inode)
        let ino = std::fs::metadata(&dest).unwrap().ino();
        install_binary(&src, &dest).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().ino(), ino);
        // No temp files left behind
        assert_eq!(std::fs::read_dir(dest.parent().unwrap()).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notices_tell_the_caller_what_to_do() {
        assert!(notice_not_finished("Dev").contains("narrater check \"Dev\""));
        assert!(notice_ask_back("Dev").contains("treat it as the reply"));
        assert!(notice_attention("Dev", "❯ 1. Yes").ends_with("❯ 1. Yes"));
    }
}
