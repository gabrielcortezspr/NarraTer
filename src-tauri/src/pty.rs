use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter, State};
use serde::{Deserialize, Serialize};

use crate::screen::{classify, plan_delivery, ScreenModel, TurnState};

const STATUS_TICK: Duration = Duration::from_millis(200);

#[derive(Serialize, Clone)]
pub struct PtyOutput {
    pub id: String,
    pub data: String,
}

#[derive(Serialize, Clone)]
pub struct PtyExit {
    pub id: String,
    pub code: u32,
}

#[derive(Serialize, Clone)]
pub struct PtyStatusEvent {
    pub id: String,
    pub status: &'static str,
}

/// One pending queue entry, as shown to the user (tile popover).
#[derive(Serialize, Clone)]
pub struct QueueItem {
    pub from_label: String,
    pub msg: String,
}

#[derive(Serialize, Clone)]
pub struct PtyQueueEvent {
    pub id: String,
    pub pending: usize,
    pub items: Vec<QueueItem>,
}

/// One inter-agent message on the ledger (per-edge history on the canvas).
#[derive(Serialize, Clone)]
pub struct LedgerEntry {
    pub from: String,
    pub to: String,
    pub from_label: String,
    pub to_label: String,
    /// "ask" | "reply" | "raw"
    pub kind: String,
    pub msg: String,
    /// Epoch milliseconds.
    pub ts: u64,
}

const LEDGER_CAP: usize = 500;

/// Prompt waiting to be typed into a terminal (delivered when it's idle).
pub struct QueuedMsg {
    pub from_label: String,
    /// Sender node id (None for prompts coming from the app itself).
    pub from_id: Option<String>,
    pub msg: String,
    /// Fired once the last byte of the prompt was written to the PTY.
    pub delivered_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

/// An `ask` in flight: `asker` is blocked waiting for `target`. If `target`
/// messages `asker` back meanwhile (the ask-back pattern), that resolves it.
pub struct AskWaiter {
    pub asker_id: String,
    pub target_id: String,
    pub tx: tokio::sync::oneshot::Sender<String>,
}

pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    child_pid: Option<u32>,
    /// Headless emulator mirroring the PTY: screen text, paste mode, activity.
    pub screen: ScreenModel,
    pub turn: TurnState,
    /// When the last turn ended (→ idle) and the screen at that moment. The
    /// next queued prompt goes in on the same tick, so an `ask` polling for
    /// idle could otherwise miss its turn's end and read the next one.
    pub last_idle: Option<(Instant, String)>,
    /// "shell" or an agent type — shells take bare commands and get their busy
    /// state from the foreground process group.
    pub agent_type: String,
    /// A prompt is being typed (multi-step write in progress).
    delivering: bool,
    /// Per-terminal secret (env NARRATER_TOKEN) proving identity on the socket.
    pub token: String,
    /// The project the terminal works on (its cwd, or the one its role
    /// directory points to) — where routine pre-run scripts run.
    pub project_dir: PathBuf,
    /// The SSH server / Docker container it runs in (None: this machine)
    pub remote: Option<crate::environments::Environment>,
    /// Its persistent tmux session (outlives the app), ended when the
    /// terminal is removed for good
    pub tmux: Option<TmuxSession>,
}

/// A terminal's tmux session: local, or in its remote environment.
#[derive(Clone)]
pub struct TmuxSession {
    pub name: String,
    pub remote: Option<crate::environments::Environment>,
}

impl TmuxSession {
    /// Ends it (in the background: a remote one goes over ssh / docker).
    pub fn end(&self) {
        let session = self.clone();
        std::thread::spawn(move || match &session.remote {
            Some(env) => {
                let _ = crate::environments::end_sessions(env, Some(&session.name));
            }
            None => {
                if let Some(tmux) = crate::environments::local_tmux() {
                    crate::environments::local_kill(&tmux, &session.name);
                }
            }
        });
    }
}

impl PtySession {
    pub fn is_shell(&self) -> bool {
        self.agent_type == "shell"
    }

    /// Shells only: is a job other than the shell itself in the foreground?
    fn foreground_busy(&self) -> Option<bool> {
        let leader = self.master.process_group_leader()?;
        let pid = self.child_pid? as i32;
        Some(leader != pid)
    }

    fn compute_turn(&self, now: Instant) -> TurnState {
        let since = now.saturating_duration_since(self.screen.last_change);
        // Remote or inside tmux: the local foreground is ssh / docker / the
        // tmux client — only the screen tells
        let fg = if self.is_shell() && self.remote.is_none() && self.tmux.is_none() { self.foreground_busy() } else { None };
        classify(&self.screen.footer(), since, self.is_shell(), fg)
    }
}

#[derive(Default)]
pub struct PtyStateInner {
    pub sessions: HashMap<String, PtySession>,
    /// Terminal names, unique per scene (several workspaces run at once)
    pub names: crate::names::Names,
    /// Cross-workspace links (`recruit --workspace`), persisted
    pub links: crate::names::Links,
    /// In-flight asks, keyed by a request id.
    pub ask_waiters: HashMap<String, AskWaiter>,
    /// Agent ropes mirrored from the canvas. Undirected: an edge between two
    /// terminals lets either side `ask` the other.
    pub connections: HashSet<(String, String)>,
    /// Per-terminal prompt queue, drained when the terminal is idle.
    pub inbox: HashMap<String, VecDeque<QueuedMsg>>,
    /// In-flight canvas requests (CLI → frontend), resolved by canvas_respond.
    pub canvas_waiters: HashMap<String, tokio::sync::oneshot::Sender<String>>,
    /// Latest inter-agent messages — per-edge conversation history.
    pub ledger: VecDeque<LedgerEntry>,
    /// Terminals in Maestro mode (mirrored from the canvas) — may manage routines.
    pub maestros: HashSet<String>,
    /// Remote terminals still starting (connecting, setting up): asks wait
    pub booting: HashSet<String>,
}

impl PtyStateInner {
    pub fn connected(&self, a: &str, b: &str) -> bool {
        self.connections.contains(&(a.to_string(), b.to_string()))
            || self.connections.contains(&(b.to_string(), a.to_string()))
            || self.links.linked(a, b)
    }

    pub fn label_of(&self, id: &str) -> String {
        self.names.label_of(id)
    }

    /// How `viewer` addresses `id` (`Name`, or `Name @ Workspace` across scenes).
    pub fn address(&self, id: &str, viewer: &str) -> String {
        self.names.address(id, viewer)
    }
}

#[derive(Clone)]
pub struct PtyState(pub Arc<Mutex<PtyStateInner>>);

impl Default for PtyState {
    fn default() -> Self {
        PtyState(Arc::new(Mutex::new(PtyStateInner { links: crate::names::Links::load(), ..Default::default() })))
    }
}

/// Role context written as instruction files the agent loads by itself
/// (see roles::install_role_dir).
#[derive(Deserialize, Clone)]
pub struct RoleSpec {
    /// Directory key under `.narrater/roles/` (role id, or the terminal id
    /// when the instructions were customized for this terminal).
    pub key: String,
    pub name: Option<String>,
    pub instructions: String,
    #[serde(default)]
    pub orchestrator: bool,
}

/// Session markers of whatever launched NarraTer (a Claude Code terminal, a
/// Maestri canvas…). Inherited by our terminals they would tie each agent to
/// the parent session (e.g. Claude disables transcript saving in "child
/// sessions") or point its CLI at another app's socket.
const INHERITED_SESSION_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];
const INHERITED_SESSION_PREFIXES: &[&str] = &["MAESTRI_"];

fn is_inherited_session_var(name: &str) -> bool {
    INHERITED_SESSION_VARS.contains(&name) || INHERITED_SESSION_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Expands `~` and falls back to $HOME when the directory doesn't exist.
fn resolve_cwd(cwd: Option<&str>) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let Some(raw) = cwd.map(str::trim).filter(|s| !s.is_empty()) else { return home };
    let path = if raw == "~" {
        home.clone()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    };
    if path.is_dir() { path } else { home }
}

/// Pre-accepts Claude Code's folder-trust dialog for `dir` (and, when spawning
/// with --dangerously-skip-permissions, its one-time bypass warning) in
/// ~/.claude.json, so agent terminals boot straight into the prompt. Written
/// atomically (temp file + rename): Claude itself rewrites this file, and a
/// truncated read would wipe the user's settings.
fn pre_trust_claude_dir(dir: &std::path::Path, bypass_permissions: bool) {
    let Some(home) = dirs::home_dir() else { return };
    let config_path = home.join(".claude.json");
    let dir = dir.to_string_lossy().to_string();

    // A corrupt/unreadable existing config is left untouched — never clobber.
    let mut root: serde_json::Value = match std::fs::read_to_string(&config_path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => return,
        },
        Err(_) => serde_json::json!({}),
    };

    let Some(obj) = root.as_object_mut() else { return };
    let mut changed = false;

    let projects = obj.entry("projects").or_insert_with(|| serde_json::json!({}));
    if let Some(projects) = projects.as_object_mut() {
        let project = projects.entry(dir).or_insert_with(|| serde_json::json!({}));
        if let Some(project) = project.as_object_mut() {
            if project.get("hasTrustDialogAccepted").and_then(|v| v.as_bool()) != Some(true) {
                project.insert("hasTrustDialogAccepted".into(), serde_json::json!(true));
                changed = true;
            }
        }
    }

    if bypass_permissions
        && obj.get("bypassPermissionsModeAccepted").and_then(|v| v.as_bool()) != Some(true)
    {
        obj.insert("bypassPermissionsModeAccepted".into(), serde_json::json!(true));
        changed = true;
    }

    if changed {
        if let Ok(json) = serde_json::to_string(&root) {
            let tmp = config_path.with_extension(format!("json.narrater-{}", std::process::id()));
            if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &config_path).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

/// Extracts the valid UTF-8 prefix of `buf`, leaving an incomplete trailing
/// sequence (at most 3 bytes) behind for the next PTY read.
fn drain_utf8(buf: &mut Vec<u8>) -> String {
    match std::str::from_utf8(buf) {
        Ok(_) => String::from_utf8(std::mem::take(buf)).unwrap(),
        Err(e) if e.error_len().is_none() && buf.len() - e.valid_up_to() < 4 => {
            let tail = buf.split_off(e.valid_up_to());
            let head = std::mem::replace(buf, tail);
            String::from_utf8(head).unwrap()
        }
        Err(_) => String::from_utf8_lossy(&std::mem::take(buf)).into_owned(),
    }
}

/// A terminal of a remote workspace, ready to start: the local command
/// (ssh / docker exec) and where the project is over there.
struct RemoteLaunch {
    argv: Vec<String>,
    project_dir: String,
    env: crate::environments::Environment,
}

/// Prepares a remote terminal: the bridge address it will use, setup (CLI
/// wrapper, skills — once per environment), its role files and launcher.
#[allow(clippy::too_many_arguments)]
async fn prepare_remote(
    env: crate::environments::Environment,
    id: &str,
    token: &str,
    label: &str,
    scene: &str,
    cwd: Option<String>,
    role: Option<RoleSpec>,
    program: String,
    args: Vec<String>,
) -> Result<RemoteLaunch, String> {
    use crate::environments::{self as envs, Environment};
    let port = crate::bridge::port().ok_or("the bridge isn't running yet — try again in a moment")?;
    // How the remote CLI reaches the bridge
    let (bridge, socket) = match &env {
        Environment::Ssh { .. } => {
            let sock = format!("/tmp/narrater-{}.sock", &uuid::Uuid::new_v4().simple().to_string()[..12]);
            (format!("unix:{}", sock), Some(sock))
        }
        _ => {
            let probe = env.clone();
            let gateway = tauri::async_runtime::spawn_blocking(move || envs::docker_gateway(&probe))
                .await
                .map_err(|e| e.to_string())??;
            match gateway {
                Some(ip) => {
                    crate::bridge::ensure_listener(&ip).await?;
                    (format!("tcp:{}:{}", ip, port), None)
                }
                None => (format!("tcp:127.0.0.1:{}", port), None),
            }
        }
    };
    let (id_s, token_s, label_s, scene_s) = (id.to_string(), token.to_string(), label.to_string(), scene.to_string());
    let env2 = env.clone();
    let project_dir = tauri::async_runtime::spawn_blocking(move || {
        let info = envs::ensure_setup(&env2, &crate::ipc::skills(), false)?;
        envs::prepare_launch(&env2, &info, &envs::Launch {
            id: &id_s,
            token: &token_s,
            label: &label_s,
            scene: &scene_s,
            cwd: cwd.as_deref(),
            role: role.as_ref(),
            program: &program,
            args: &args,
            bridge: &bridge,
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(RemoteLaunch { argv: envs::pty_command(&env, id, socket.as_deref(), port), project_dir, env })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn(
    id: String,
    command: String,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
    label: Option<String>,
    agent_type: Option<String>,
    cwd: Option<String>,
    role: Option<RoleSpec>,
    scene: Option<String>,
    app_handle: AppHandle,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let bypass_permissions = args.as_ref().is_some_and(|a| {
        a.iter().any(|s| s == "--dangerously-skip-permissions")
    });

    // With explicit args, `command` is the program; otherwise fall back to
    // whitespace-splitting the command string (custom agents, no quoting)
    let (program, prog_args): (String, Vec<String>) = match args {
        Some(args) => (command.clone(), args),
        None => {
            let mut parts = command.split_whitespace();
            let program = parts.next().ok_or("empty command")?.to_string();
            (program, parts.map(str::to_string).collect())
        }
    };
    let agent_type = agent_type.unwrap_or_else(|| "shell".to_string());
    let role = role.filter(|r| agent_type != "shell" && !r.instructions.trim().is_empty());

    // Labels address agents in `narrater ask`; disambiguate duplicates in
    // the scene so two "claude" terminals never clobber each other's route.
    // Reserved before spawn so the child sees its own identity in the
    // environment.
    let scene = scene.unwrap_or_default();
    let effective_label = {
        let mut inner = state.0.lock().unwrap();
        let base = label.unwrap_or_else(|| id.clone());
        let name = inner.names.set(&id, &scene, &base);
        inner.links.refresh_label(&id, &name);
        name
    };
    let token = uuid::Uuid::new_v4().simple().to_string();

    // Swapping the agent in place (same id, `recruit --replace`, role
    // change): its old persistent session must go, or the new spawn would
    // reattach to the old agent
    let old_tmux = state.0.lock().unwrap().sessions.get(&id).and_then(|s| s.tmux.clone());
    if let Some(old) = old_tmux {
        match &old.remote {
            Some(env) => {
                let (env, name) = (env.clone(), old.name.clone());
                let _ = tauri::async_runtime::spawn_blocking(move || crate::environments::end_sessions(&env, Some(&name))).await;
            }
            None => {
                if let Some(tmux) = crate::environments::local_tmux() {
                    crate::environments::local_kill(&tmux, &old.name);
                }
            }
        }
    }

    // A workspace on an SSH server or in a Docker container: the terminal is
    // ssh / docker exec running the agent over there
    let remote = match crate::workspaces::environment_of(&scene) {
        Some(env) => {
            state.0.lock().unwrap().booting.insert(id.clone());
            let prepared = prepare_remote(
                env, &id, &token, &effective_label, &scene, cwd.clone(), role.clone(), program.clone(), prog_args.clone(),
            )
            .await;
            if prepared.is_err() {
                state.0.lock().unwrap().booting.remove(&id);
            }
            match prepared {
                Ok(r) => Some(r),
                Err(e) => {
                    state.0.lock().unwrap().names.remove(&id);
                    return Err(e);
                }
            }
        }
        None => None,
    };

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let key = crate::environments::session_key(&id);
    let mut tmux_session = None;
    let (mut cmd, project_dir, spawn_dir) = match &remote {
        Some(r) => {
            let mut cmd = CommandBuilder::new(&r.argv[0]);
            cmd.args(&r.argv[1..]);
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            if r.env.persistent() {
                tmux_session = Some(TmuxSession { name: format!("narrater-{}", key), remote: Some(r.env.clone()) });
            }
            (cmd, PathBuf::from(&r.project_dir), home)
        }
        None => {
            let mut cmd = CommandBuilder::new(&program);
            cmd.args(&prog_args);
            // Working directory: the project the terminal works on. With a
            // role, the agent starts inside `.narrater/roles/<key>/` so it
            // loads the role's CLAUDE.md / AGENTS.md natively — the files
            // point back to the project.
            let project_dir = resolve_cwd(cwd.as_deref());
            let spawn_dir = match role.as_ref() {
                Some(r) => crate::roles::install_role_dir(&project_dir, r, &command).unwrap_or_else(|e| {
                    eprintln!("[NarraTer] {}", e);
                    project_dir.clone()
                }),
                None => project_dir.clone(),
            };
            // Persistent: the agent runs inside our tmux server, so it keeps
            // running when NarraTer closes; reopening reattaches (`-A`)
            let tmux = crate::preferences::get().persistent_sessions.then(crate::environments::local_tmux).flatten();
            let launched = tmux.and_then(|tmux| {
                let bin = dirs::home_dir().map(|h| h.join(".local/bin")).unwrap_or_default();
                let cli = bin.join("narrater").display().to_string();
                let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
                let socket = crate::ipc::socket_path();
                let exports = [
                    ("NARRATER_SOCKET", socket.as_str()),
                    ("NARRATER_ID", id.as_str()),
                    ("NARRATER_TOKEN", token.as_str()),
                    ("NARRATER_LABEL", effective_label.as_str()),
                    ("NARRATER_SCENE", scene.as_str()),
                    ("NARRATER_KEY", key.as_str()),
                    ("NARRATER_CLI", cli.as_str()),
                    ("PATH", path.as_str()),
                    ("TERM_PROGRAM", "NarraTer"),
                    ("COLORTERM", "truecolor"),
                ];
                let argv: Vec<String> = std::iter::once(program.clone()).chain(prog_args.iter().cloned()).collect();
                let run = crate::environments::local_launch(&key, &exports, &spawn_dir, &argv)
                    .map_err(|e| eprintln!("[NarraTer] persistent session unavailable: {}", e))
                    .ok()?;
                crate::environments::local_publish(&tmux, &key, &socket, &token);
                let name = format!("narrater-{}", key);
                let mut cmd = CommandBuilder::new(&tmux);
                cmd.args(["-L", crate::environments::LOCAL_TMUX_SOCKET, "-f"]);
                cmd.arg(crate::environments::local_tmux_conf());
                cmd.args(["new-session", "-A", "-s", name.as_str(), "sh"]);
                cmd.arg(&run);
                Some((cmd, name))
            });
            match launched {
                Some((tmux_cmd, name)) => {
                    tmux_session = Some(TmuxSession { name, remote: None });
                    (tmux_cmd, project_dir, spawn_dir)
                }
                None => (cmd, project_dir, spawn_dir),
            }
        }
    };
    cmd.cwd(&spawn_dir);

    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_inherited_session_var) {
            cmd.env_remove(name);
        }
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "NarraTer");

    if let Some(home) = dirs::home_dir() {
        cmd.env("HOME", &home);
        let bin = home.join(".local/bin");
        match std::env::var("PATH") {
            Ok(path) => cmd.env("PATH", format!("{}:{}", bin.display(), path)),
            Err(_) => cmd.env("PATH", bin.display().to_string()),
        }
        cmd.env("NARRATER_CLI", bin.join("narrater").display().to_string());
    }

    cmd.env("NARRATER_SOCKET", crate::ipc::socket_path());
    cmd.env("NARRATER_SCENE", &scene);
    cmd.env("NARRATER_ID", &id);
    cmd.env("NARRATER_LABEL", &effective_label);
    cmd.env("NARRATER_TOKEN", &token);

    if remote.is_none() && (command == "claude" || command.ends_with("/claude")) {
        // No interactive dialogs on boot: the user creating the terminal in
        // NarraTer is the trust decision.
        pre_trust_claude_dir(&spawn_dir, bypass_permissions);
    }

    let child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            state.0.lock().unwrap().names.remove(&id);
            return Err(e.to_string());
        }
    };
    drop(pair.slave);
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    {
        let mut inner = state.0.lock().unwrap();
        // Same id already running = the node swaps its agent in place
        // (`narrater recruit --replace`, `role assign`). Its connections and
        // label stay; the old process goes, and so do asks waiting on it —
        // they'd otherwise end up reading the new agent's screen.
        if let Some(mut old) = inner.sessions.remove(&id) {
            let _ = old.child.kill();
            std::thread::spawn(move || {
                let _ = old.child.wait();
            });
            inner.ask_waiters.retain(|_, w| w.target_id != id);
        }
        inner.booting.remove(&id);
        inner.sessions.insert(id.clone(), PtySession {
            writer: Box::new(writer),
            master: pair.master,
            child_pid: child.process_id(),
            child,
            screen: ScreenModel::new(rows, cols),
            turn: TurnState::Working,
            last_idle: None,
            agent_type,
            delivering: false,
            token: token.clone(),
            project_dir: project_dir.clone(),
            remote: remote.as_ref().map(|r| r.env.clone()),
            tmux: tmux_session,
        });
    }

    let id_clone = id.clone();
    let app_clone = app_handle.clone();
    let state_arc = Arc::clone(&state.0);

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let child = {
                        let mut inner = state_arc.lock().unwrap();
                        // Superseded by a respawn under the same id: the new
                        // session owns the id now — nothing to clean, no exit
                        // event (the frontend would mark the new one dead).
                        if !inner.sessions.get(&id_clone).is_some_and(|s| s.token == token) {
                            break;
                        }
                        // Dropping the senders wakes pending asks with a clean error
                        inner.ask_waiters.retain(|_, w| w.target_id != id_clone && w.asker_id != id_clone);
                        inner.inbox.remove(&id_clone);
                        inner.names.remove(&id_clone);
                        inner.sessions.remove(&id_clone).map(|s| s.child)
                    };
                    let code = child
                        .and_then(|mut c| c.wait().ok())
                        .map(|status| status.exit_code())
                        .unwrap_or(0);
                    let _ = app_clone.emit("pty_exit", PtyExit { id: id_clone, code });
                    break;
                }
                Ok(n) => {
                    if let Some(s) = state_arc.lock().unwrap().sessions.get_mut(&id_clone) {
                        s.screen.feed(&buf[..n]);
                    }
                    carry.extend_from_slice(&buf[..n]);
                    let data = drain_utf8(&mut carry);
                    if !data.is_empty() {
                        let _ = app_clone.emit("pty_output", PtyOutput { id: id_clone.clone(), data });
                    }
                }
            }
        }
    });

    Ok(effective_label)
}

#[tauri::command]
pub fn pty_write(id: String, data: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    let Some(session) = inner.sessions.get_mut(&id) else { return Ok(()) };
    session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(id: String, cols: u16, rows: u16, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    if let Some(session) = inner.sessions.get_mut(&id) {
        session.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        session.screen.resize(rows, cols);
    }
    Ok(())
}

#[tauri::command]
pub fn pty_kill(id: String, state: State<'_, PtyState>) -> Result<(), String> {
    // Kill the child but keep the session in the map: the reader thread sees
    // EOF, reaps the real exit code and does the full cleanup there.
    let mut inner = state.0.lock().unwrap();
    if let Some(session) = inner.sessions.get_mut(&id) {
        // Removed for good: a persistent session goes too (closing the app
        // doesn't come through here, so agents outlive it)
        if let Some(t) = &session.tmux {
            t.end();
        }
        session.child.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_update_label(id: String, label: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    inner.names.rename(&id, &label)?;
    inner.links.refresh_label(&id, &label);
    Ok(())
}

#[tauri::command]
pub fn connections_sync(connections: Vec<(String, String)>, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    inner.connections = connections.into_iter().collect();
    Ok(())
}

/// Terminals in Maestro mode — commands only a Maestro may run check it.
#[tauri::command]
pub fn maestros_sync(ids: Vec<String>, state: State<'_, PtyState>) -> Result<(), String> {
    state.0.lock().unwrap().maestros = ids.into_iter().collect();
    Ok(())
}

/// Current turn state of each live session (the frontend re-syncs on load).
#[tauri::command]
pub fn pty_statuses(state: State<'_, PtyState>) -> HashMap<String, &'static str> {
    let inner = state.0.lock().unwrap();
    inner.sessions.iter().map(|(id, s)| (id.clone(), s.turn.as_str())).collect()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Records an inter-agent message on the ledger (ring buffer) and notifies
/// the front via `narrater_msg` — feeds the history and the edge pulses.
pub fn record_msg(state_arc: &Arc<Mutex<PtyStateInner>>, app: &AppHandle, entry: LedgerEntry) {
    {
        let mut inner = state_arc.lock().unwrap();
        inner.ledger.push_back(entry.clone());
        if inner.ledger.len() > LEDGER_CAP {
            inner.ledger.pop_front();
        }
    }
    let _ = app.emit("narrater_msg", entry);
}

pub(crate) fn queue_snapshot(inner: &PtyStateInner, id: &str) -> Vec<QueueItem> {
    inner
        .inbox
        .get(id)
        .map(|q| {
            q.iter()
                .map(|m| QueueItem { from_label: m.from_label.clone(), msg: m.msg.clone() })
                .collect()
        })
        .unwrap_or_default()
}

/// Conversation history between two nodes (both directions), for the panel
/// opened by clicking an agent edge.
#[tauri::command]
pub fn narrater_ledger(a: String, b: String, state: State<'_, PtyState>) -> Vec<LedgerEntry> {
    let inner = state.0.lock().unwrap();
    inner
        .ledger
        .iter()
        .filter(|e| (e.from == a && e.to == b) || (e.from == b && e.to == a))
        .cloned()
        .collect()
}

/// Cancels a queued prompt (index into terminal `id`'s queue). If it was an
/// ask, dropping the delivered_tx wakes the caller with a delivery error.
#[tauri::command]
pub fn pty_queue_cancel(
    id: String,
    index: usize,
    app_handle: AppHandle,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    let items = {
        let mut inner = state.0.lock().unwrap();
        if let Some(queue) = inner.inbox.get_mut(&id) {
            if index >= queue.len() {
                return Err("index outside the queue".to_string());
            }
            queue.remove(index);
            if queue.is_empty() {
                inner.inbox.remove(&id);
            }
        }
        queue_snapshot(&inner, &id)
    };
    let _ = app_handle.emit("pty_queue", PtyQueueEvent { id, pending: items.len(), items });
    Ok(())
}

/// Enqueues a prompt for idle-gated delivery into `target_id`.
/// Returns the queue position (0 = next to be delivered).
pub fn enqueue_message(
    state_arc: &Arc<Mutex<PtyStateInner>>,
    app: &AppHandle,
    target_id: &str,
    msg: QueuedMsg,
) -> usize {
    let (pending, items) = {
        let mut inner = state_arc.lock().unwrap();
        let queue = inner.inbox.entry(target_id.to_string()).or_default();
        queue.push_back(msg);
        let pending = queue.len();
        (pending, queue_snapshot(&inner, target_id))
    };
    let _ = app.emit("pty_queue", PtyQueueEvent { id: target_id.to_string(), pending, items });
    pending - 1
}

/// Withdraws a still-undelivered prompt (an ask that gave up waiting).
pub fn withdraw_message(
    state_arc: &Arc<Mutex<PtyStateInner>>,
    app: &AppHandle,
    target_id: &str,
    pred: impl Fn(&QueuedMsg) -> bool,
) {
    let items = {
        let mut inner = state_arc.lock().unwrap();
        if let Some(queue) = inner.inbox.get_mut(target_id) {
            queue.retain(|m| !pred(m));
            if queue.is_empty() {
                inner.inbox.remove(target_id);
            }
        }
        queue_snapshot(&inner, target_id)
    };
    let _ = app.emit("pty_queue", PtyQueueEvent { id: target_id.to_string(), pending: items.len(), items });
}

/// `[narrater from X]: msg` — the prompt as an agent sees it. Shells get the
/// bare command (a frame would break it).
fn frame_message(qmsg: &QueuedMsg, is_shell: bool) -> String {
    if is_shell || qmsg.from_id.is_none() {
        qmsg.msg.clone()
    } else {
        format!("[narrater from {}]: {}", qmsg.from_label, qmsg.msg)
    }
}

/// Background task: refreshes every session's screen model, derives its turn
/// state (notifying the frontend on each transition) and types the next
/// queued prompt into terminals that are idle.
pub async fn start_status_monitor(app: AppHandle, state: Arc<Mutex<PtyStateInner>>) {
    let mut interval = tokio::time::interval(STATUS_TICK);
    loop {
        interval.tick().await;
        let now = Instant::now();

        let mut transitions: Vec<(String, TurnState)> = Vec::new();
        let mut deliveries: Vec<(String, QueuedMsg, Vec<QueueItem>, bool, bool)> = Vec::new();
        {
            let mut guard = state.lock().unwrap();
            let inner = &mut *guard;
            for (id, s) in inner.sessions.iter_mut() {
                s.screen.refresh(now);
                let turn = if s.delivering { TurnState::Working } else { s.compute_turn(now) };
                if turn != s.turn {
                    s.turn = turn;
                    if turn == TurnState::Idle {
                        s.last_idle = Some((now, s.screen.text()));
                    }
                    transitions.push((id.clone(), turn));
                }
            }

            // At most one prompt per terminal per tick, only into idle ones
            let ids: Vec<String> = inner.inbox.keys().cloned().collect();
            for id in ids {
                let Some(session) = inner.sessions.get_mut(&id) else {
                    inner.inbox.remove(&id);
                    continue;
                };
                if session.turn != TurnState::Idle || session.delivering || !session.screen.has_output {
                    continue;
                }
                let Some(queue) = inner.inbox.get_mut(&id) else { continue };
                let Some(msg) = queue.pop_front() else { continue };
                if queue.is_empty() {
                    inner.inbox.remove(&id);
                }
                // Working right away: the screen hasn't reacted yet, and the
                // settle window restarts from the delivery
                session.delivering = true;
                session.turn = TurnState::Working;
                session.screen.last_change = now;
                transitions.push((id.clone(), TurnState::Working));
                let is_shell = session.is_shell();
                let bracketed = session.screen.bracketed_paste();
                let items = queue_snapshot(inner, &id);
                deliveries.push((id, msg, items, is_shell, bracketed));
            }
        }

        for (id, turn) in transitions {
            let _ = app.emit("pty_status", PtyStatusEvent { id, status: turn.as_str() });
        }

        for (id, mut qmsg, items, is_shell, bracketed) in deliveries {
            let steps = plan_delivery(&frame_message(&qmsg, is_shell), bracketed, is_shell);
            let state_clone = Arc::clone(&state);
            let delivered_tx = qmsg.delivered_tx.take();
            let id_clone = id.clone();
            tokio::spawn(async move {
                for step in steps {
                    if step.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(step.delay_ms)).await;
                    }
                    write_to_pty(&state_clone, &id_clone, &step.data);
                }
                if let Some(s) = state_clone.lock().unwrap().sessions.get_mut(&id_clone) {
                    s.delivering = false;
                    s.screen.last_change = Instant::now();
                }
                if let Some(tx) = delivered_tx {
                    let _ = tx.send(());
                }
            });
            let _ = app.emit("pty_queue", PtyQueueEvent { id, pending: items.len(), items });
        }
    }
}

pub fn write_to_pty(state_arc: &Arc<Mutex<PtyStateInner>>, id: &str, data: &str) -> bool {
    let mut inner = state_arc.lock().unwrap();
    if let Some(session) = inner.sessions.get_mut(id) {
        let _ = session.writer.write_all(data.as_bytes());
        let _ = session.writer.flush();
        // Raw keystrokes restart the settle window like a delivery does
        session.screen.last_change = Instant::now();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qmsg(from: Option<&str>, msg: &str) -> QueuedMsg {
        QueuedMsg {
            from_label: from.unwrap_or("app").to_string(),
            from_id: from.map(String::from),
            msg: msg.to_string(),
            delivered_tx: None,
        }
    }

    #[test]
    fn frames_agent_prompts_but_not_shell_commands() {
        assert_eq!(frame_message(&qmsg(Some("planner"), "hi"), false), "[narrater from planner]: hi");
        assert_eq!(frame_message(&qmsg(Some("planner"), "ls"), true), "ls");
        assert_eq!(frame_message(&qmsg(None, "scheduled"), false), "scheduled");
    }


    #[test]
    fn connections_are_undirected() {
        let inner = PtyStateInner {
            connections: [("a".to_string(), "b".to_string())].into_iter().collect(),
            ..Default::default()
        };
        assert!(inner.connected("a", "b"));
        assert!(inner.connected("b", "a"));
        assert!(!inner.connected("a", "c"));
    }

    #[test]
    fn parent_session_markers_are_not_inherited() {
        assert!(is_inherited_session_var("CLAUDE_CODE_CHILD_SESSION"));
        assert!(is_inherited_session_var("MAESTRI_SOCKET"));
        // Configuration stays (provider, model, API keys…)
        assert!(!is_inherited_session_var("CLAUDE_CODE_USE_BEDROCK"));
        assert!(!is_inherited_session_var("ANTHROPIC_API_KEY"));
        assert!(!is_inherited_session_var("PATH"));
    }

    #[test]
    fn resolve_cwd_expands_home_and_falls_back() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve_cwd(None), home);
        assert_eq!(resolve_cwd(Some("~")), home);
        assert_eq!(resolve_cwd(Some("/definitely/not/here")), home);
        assert_eq!(resolve_cwd(Some("/tmp")), PathBuf::from("/tmp"));
    }
}
