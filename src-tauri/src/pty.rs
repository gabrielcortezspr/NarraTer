use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter, State};
use serde::Serialize;
use tokio::sync::mpsc;

/// Silence window after which a Running terminal is considered Idle.
pub const IDLE_THRESHOLD: Duration = Duration::from_millis(1500);
const STATUS_TICK: Duration = Duration::from_millis(500);
/// A queued message is injected even into a busy terminal after this long,
/// so TUI agents that never go idle (constant redraws) don't starve the queue.
pub const MAX_QUEUE_WAIT: Duration = Duration::from_secs(30);
/// Force-inject window for sessions with a working idle hook: the Stop hook
/// is authoritative, so the queue only needs a long safety backstop.
pub const HOOKED_QUEUE_WAIT: Duration = Duration::from_secs(120);
/// Receiving a message from A lets the target answer A back (send/ask) for
/// this long, even without a canvas edge in that direction.
pub const REPLY_GRANT_TTL: Duration = Duration::from_secs(600);

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
    pub msg_id: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct PtyQueueEvent {
    pub id: String,
    pub pending: usize,
    pub items: Vec<QueueItem>,
}

/// One inter-agent message on the ledger (fase 4 — observabilidade). Ring
/// buffer global; o front recebe cada registro via evento `narrater_msg`.
#[derive(Serialize, Clone)]
pub struct LedgerEntry {
    pub from: String,
    pub to: String,
    pub from_label: String,
    pub to_label: String,
    /// "send" | "ask" | "reply" | "broadcast"
    pub kind: String,
    pub msg: String,
    pub msg_id: Option<String>,
    /// Epoch em milissegundos.
    pub ts: u64,
}

const LEDGER_CAP: usize = 500;

/// Message waiting to be injected into a terminal's stdin.
pub struct QueuedMsg {
    pub from_label: String,
    /// Sender node id, when the sender is another agent (None for "sistema").
    /// Delivery grants the receiver a temporary reply route back to it.
    pub from_id: Option<String>,
    pub msg: String,
    pub enqueued: Instant,
    /// Short ask id. When present, delivery frames the message as
    /// `[narrater de X #id]: ...` so the target can answer via `reply`.
    pub msg_id: Option<String>,
    /// Fired when the message is actually written to the target PTY, so an
    /// `ask` starts capturing output only from that point.
    pub delivered_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

/// One in-flight `ask` capture, keyed by request id in the state map.
/// Fallback path for shell targets — AI agents answer via `reply`.
pub struct ResponseListener {
    pub target_id: String,
    pub tx: mpsc::Sender<String>,
}

/// One in-flight explicit-reply `ask`, keyed by short msg id. Resolved by the
/// target agent calling `narrater reply <id>` / the reply_message MCP tool.
pub struct AskWaiter {
    /// Only this terminal may answer — the one the ask was delivered to.
    pub responder_id: String,
    /// Who asked — destination of the reply on the ledger.
    pub asker_id: String,
    pub tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RunStatus {
    Running,
    Idle,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Idle => "idle",
        }
    }
}

pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    pub last_output: Instant,
    pub status: RunStatus,
    /// Agent type running in this terminal ("shell", "claude", ...). Decides
    /// how inbound messages are framed on delivery.
    pub agent_type: String,
    /// True once this session's Stop hook called `narrater notify-idle` at
    /// least once — from then on, idle comes from the hook (authoritative),
    /// not from the output-silence timer.
    pub hook_idle: bool,
    /// Segredo por terminal (env NARRATER_TOKEN), validado pelo IPC junto ao
    /// `from` — fecha o spoof de identidade por qualquer processo local.
    pub token: String,
}

pub struct PtyStateInner {
    pub sessions: HashMap<String, PtySession>,
    pub labels: HashMap<String, String>,
    pub label_to_id: HashMap<String, String>,
    /// In-flight ask captures, keyed by request id (concurrent asks coexist).
    pub response_listeners: HashMap<String, ResponseListener>,
    /// In-flight explicit-reply asks, keyed by short msg id.
    pub ask_waiters: HashMap<String, AskWaiter>,
    /// Directed agent-pipe routes (source node id → target node id), mirrored
    /// from the canvas edges. Communication is only allowed along these.
    pub connections: HashSet<(String, String)>,
    /// Temporary reply routes (receiver id → sender id, granted on message
    /// delivery, expire after REPLY_GRANT_TTL). Let an agent answer whoever
    /// messaged it even when the canvas edge is one-way.
    pub reply_grants: HashMap<(String, String), Instant>,
    /// Per-terminal inbound message queue, drained on idle by the monitor.
    pub inbox: HashMap<String, VecDeque<QueuedMsg>>,
    /// In-flight canvas requests (MCP → frontend), keyed by request id; the
    /// frontend resolves them via the canvas_respond command.
    pub canvas_waiters: HashMap<String, tokio::sync::oneshot::Sender<String>>,
    /// Ring buffer with the latest inter-agent messages (send/ask/reply/
    /// broadcast) — per-edge conversation history on the canvas.
    pub ledger: VecDeque<LedgerEntry>,
}

#[derive(Clone)]
pub struct PtyState(pub Arc<Mutex<PtyStateInner>>);

impl Default for PtyState {
    fn default() -> Self {
        PtyState(Arc::new(Mutex::new(PtyStateInner {
            sessions: HashMap::new(),
            labels: HashMap::new(),
            label_to_id: HashMap::new(),
            response_listeners: HashMap::new(),
            ask_waiters: HashMap::new(),
            connections: HashSet::new(),
            reply_grants: HashMap::new(),
            inbox: HashMap::new(),
            canvas_waiters: HashMap::new(),
            ledger: VecDeque::new(),
        })))
    }
}

/// Pre-accepts Claude Code's folder-trust dialog for the spawn cwd (and, when
/// spawning with --dangerously-skip-permissions, its one-time bypass warning)
/// in ~/.claude.json, so agent terminals boot straight into the prompt.
fn pre_trust_claude_cwd(bypass_permissions: bool) {
    let Ok(home) = std::env::var("HOME") else { return };
    let config_path = std::path::Path::new(&home).join(".claude.json");
    let Ok(cwd) = std::env::current_dir() else { return };
    let cwd = cwd.to_string_lossy().to_string();

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
        let project = projects.entry(cwd).or_insert_with(|| serde_json::json!({}));
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
            let _ = std::fs::write(&config_path, json);
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

#[tauri::command]
pub fn pty_spawn(
    id: String,
    command: String,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
    label: Option<String>,
    agent_type: Option<String>,
    app_handle: AppHandle,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let bypass_permissions = args
        .as_ref()
        .is_some_and(|a| a.iter().any(|s| s == "--dangerously-skip-permissions"));

    // With explicit args, `command` is the program; otherwise fall back to
    // whitespace-splitting the command string (custom agents, no quoting)
    let mut cmd = match args {
        Some(args) => {
            let mut cmd = CommandBuilder::new(&command);
            for arg in args {
                cmd.arg(arg);
            }
            cmd
        }
        None => {
            let mut parts = command.split_whitespace();
            let program = parts.next().ok_or("empty command")?;
            let mut cmd = CommandBuilder::new(program);
            for arg in parts {
                cmd.arg(arg);
            }
            cmd
        }
    };
    cmd.env("TERM", "xterm-256color");
    // Generous MCP tool-call timeout so long ask_agent calls don't get cut
    cmd.env("MCP_TOOL_TIMEOUT", "300000");
    cmd.env("COLORTERM", "truecolor");

    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", &home);
        if let Ok(current_path) = std::env::var("PATH") {
            cmd.env("PATH", format!("{}/.local/bin:{}", home, current_path));
        }
    } else if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }

    let socket_path = format!("/tmp/narrater-{}.sock", std::process::id());
    cmd.env("NARRATER_SOCKET", &socket_path);

    // Labels address agents in `narrater send`; disambiguate duplicates so
    // two "claude" terminals never clobber each other's route. Reserved before
    // spawn so the child sees its own identity in the environment.
    let effective_label = {
        let mut inner = state.0.lock().unwrap();
        let base = label.unwrap_or_else(|| id.clone());
        let candidate = dedup_label(&inner.label_to_id, &id, &base);
        inner.labels.insert(id.clone(), candidate.clone());
        inner.label_to_id.insert(candidate.clone(), id.clone());
        candidate
    };
    cmd.env("NARRATER_ID", &id);
    cmd.env("NARRATER_LABEL", &effective_label);
    let token = uuid::Uuid::new_v4().simple().to_string();
    cmd.env("NARRATER_TOKEN", &token);

    let agent_type = agent_type.unwrap_or_else(|| "shell".to_string());
    // Claude gets the Stop hook → `narrater notify-idle` (authoritative idle
    // instead of the silence timer). --settings adds to the user's settings.
    if agent_type == "claude" {
        // No interactive dialogs on boot: the user creating the terminal in
        // NarraTer is the trust decision.
        pre_trust_claude_cwd(bypass_permissions);
        if let Some(hooks) = crate::ipc::claude_hooks_path() {
            if hooks.exists() {
                cmd.arg("--settings");
                cmd.arg(hooks);
            }
        }
    }

    let spawn_result = pair.slave.spawn_command(cmd);
    let child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            let mut inner = state.0.lock().unwrap();
            inner.labels.remove(&id);
            inner.label_to_id.remove(&effective_label);
            return Err(e.to_string());
        }
    };
    drop(pair.slave);
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    {
        let mut inner = state.0.lock().unwrap();
        inner.sessions.insert(id.clone(), PtySession {
            writer: Box::new(writer),
            master: pair.master,
            child,
            last_output: Instant::now(),
            status: RunStatus::Running,
            agent_type,
            hook_idle: false,
            token,
        });
    }

    let id_clone = id.clone();
    let app_clone = app_handle.clone();
    let state_arc = Arc::clone(&state.0);

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let child = {
                        let mut inner = state_arc.lock().unwrap();
                        inner.response_listeners.retain(|_, l| l.target_id != id_clone);
                        // Dropping the sender wakes pending asks with a clean
                        // "agent exited without replying" error.
                        inner.ask_waiters.retain(|_, w| w.responder_id != id_clone);
                        inner.inbox.remove(&id_clone);
                        if let Some(label) = inner.labels.remove(&id_clone) {
                            inner.label_to_id.remove(&label);
                        }
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
                    carry.extend_from_slice(&buf[..n]);
                    let data = drain_utf8(&mut carry);
                    if data.is_empty() {
                        continue;
                    }
                    let _ = app_clone.emit("pty_output", PtyOutput { id: id_clone.clone(), data: data.clone() });

                    let (senders, became_running) = {
                        let mut inner = state_arc.lock().unwrap();
                        let mut became_running = false;
                        if let Some(session) = inner.sessions.get_mut(&id_clone) {
                            session.last_output = Instant::now();
                            if session.status == RunStatus::Idle {
                                session.status = RunStatus::Running;
                                became_running = true;
                            }
                        }
                        let senders: Vec<mpsc::Sender<String>> = inner
                            .response_listeners
                            .values()
                            .filter(|l| l.target_id == id_clone)
                            .map(|l| l.tx.clone())
                            .collect();
                        (senders, became_running)
                    };
                    if became_running {
                        let _ = app_clone.emit("pty_status", PtyStatusEvent {
                            id: id_clone.clone(),
                            status: RunStatus::Running.as_str(),
                        });
                    }
                    for tx in senders {
                        let _ = tx.try_send(data.clone());
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
    if let Some(session) = inner.sessions.get_mut(&id) {
        session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        session.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(id: String, cols: u16, rows: u16, state: State<'_, PtyState>) -> Result<(), String> {
    let inner = state.0.lock().unwrap();
    if let Some(session) = inner.sessions.get(&id) {
        session.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_kill(id: String, state: State<'_, PtyState>) -> Result<(), String> {
    // Kill the child but keep the session in the map: the reader thread sees
    // EOF, reaps the real exit code and does the full cleanup there.
    let mut inner = state.0.lock().unwrap();
    if let Some(session) = inner.sessions.get_mut(&id) {
        session.child.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_update_label(id: String, label: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    if inner.label_to_id.get(&label).is_some_and(|owner| owner != &id) {
        return Err(format!("Label '{}' is already used by another terminal", label));
    }
    if let Some(old_label) = inner.labels.get(&id).cloned() {
        inner.label_to_id.remove(&old_label);
    }
    inner.labels.insert(id.clone(), label.clone());
    inner.label_to_id.insert(label, id);
    Ok(())
}

#[tauri::command]
pub fn connections_sync(connections: Vec<(String, String)>, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    inner.connections = connections.into_iter().collect();
    Ok(())
}

/// Queues a system notification for an AI terminal — delivered idle-gated and
/// auto-submitted as `[narrater from system]: <text>`. Never use on shell
/// targets: shell delivery executes the text as a command.
#[tauri::command]
pub fn pty_notify(id: String, text: String, app_handle: AppHandle, state: State<'_, PtyState>) -> Result<(), String> {
    enqueue_message(&state.0, &app_handle, &id, QueuedMsg {
        from_label: "system".to_string(),
        from_id: None,
        msg: text,
        enqueued: Instant::now(),
        msg_id: None,
        delivered_tx: None,
    });
    Ok(())
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

fn queue_snapshot(inner: &PtyStateInner, id: &str) -> Vec<QueueItem> {
    inner
        .inbox
        .get(id)
        .map(|q| {
            q.iter()
                .map(|m| QueueItem {
                    from_label: m.from_label.clone(),
                    msg: m.msg.clone(),
                    msg_id: m.msg_id.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Conversation history between two nodes (both directions), for the panel
/// opened by clicking an agent-pipe edge.
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

/// Cancels a queued message (index into terminal `id`'s queue). If it was an
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

/// Enqueues a message for idle-gated delivery into `target_id`'s stdin.
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

/// Background task: flips Running sessions to Idle after IDLE_THRESHOLD of
/// output silence, notifies the frontend on each transition, and drains the
/// per-terminal inbox — a queued message is injected when its target is Idle
/// (or after MAX_QUEUE_WAIT, so never-idle TUIs don't starve the queue).
pub async fn start_status_monitor(app: AppHandle, state: Arc<Mutex<PtyStateInner>>) {
    let mut interval = tokio::time::interval(STATUS_TICK);
    loop {
        interval.tick().await;

        let newly_idle: Vec<String> = {
            let mut inner = state.lock().unwrap();
            inner
                .sessions
                .iter_mut()
                // Sessions with a working Stop hook go idle only via
                // `narrater notify-idle` — the silence timer would flag
                // "idle" nas pausas de output no meio de um turno longo.
                .filter(|(_, s)| {
                    !s.hook_idle
                        && s.status == RunStatus::Running
                        && s.last_output.elapsed() >= IDLE_THRESHOLD
                })
                .map(|(id, s)| {
                    s.status = RunStatus::Idle;
                    id.clone()
                })
                .collect()
        };
        for id in newly_idle {
            let _ = app.emit("pty_status", PtyStatusEvent { id, status: RunStatus::Idle.as_str() });
        }

        // Deliver at most one queued message per terminal per tick
        let deliveries: Vec<(String, QueuedMsg, Vec<QueueItem>, String)> = {
            let mut inner = state.lock().unwrap();
            let ids: Vec<String> = inner.inbox.keys().cloned().collect();
            let mut out = Vec::new();
            for id in ids {
                let Some(session) = inner.sessions.get(&id) else {
                    inner.inbox.remove(&id);
                    continue;
                };
                // With an authoritative idle hook the force-inject backstop
                // can be much longer — it only covers a broken hook.
                let force_wait = if session.hook_idle { HOOKED_QUEUE_WAIT } else { MAX_QUEUE_WAIT };
                let ready = match session.status {
                    RunStatus::Idle => true,
                    RunStatus::Running => inner
                        .inbox
                        .get(&id)
                        .and_then(|q| q.front())
                        .is_some_and(|m| m.enqueued.elapsed() >= force_wait),
                };
                if !ready {
                    continue;
                }
                if let Some(queue) = inner.inbox.get_mut(&id) {
                    if let Some(msg) = queue.pop_front() {
                        if queue.is_empty() {
                            inner.inbox.remove(&id);
                        }
                        // Mark Running right away so the whole queue isn't
                        // flushed before the injected command produces output
                        let mut agent_type = "shell".to_string();
                        if let Some(s) = inner.sessions.get_mut(&id) {
                            s.status = RunStatus::Running;
                            s.last_output = Instant::now();
                            agent_type = s.agent_type.clone();
                        }
                        // Receiving a message from A authorizes replying to A
                        // for REPLY_GRANT_TTL, even without a return edge.
                        if let Some(from_id) = msg.from_id.clone() {
                            inner.reply_grants.insert((id.clone(), from_id), Instant::now());
                            inner.reply_grants.retain(|_, t| t.elapsed() < REPLY_GRANT_TTL);
                        }
                        let items = queue_snapshot(&inner, &id);
                        out.push((id.clone(), msg, items, agent_type));
                    }
                }
            }
            out
        };

        for (id, mut qmsg, items, agent_type) in deliveries {
            // \r submits the line (terminals send Enter as carriage return —
            // \n is not enough for raw-mode TUIs). Shells get the bare command
            // (the sender frame would break it) preceded by kill-line (^U) to
            // clear any half-typed input; AI agents get the sender frame.
            match agent_type.as_str() {
                "shell" => {
                    write_to_pty(&state, &id, &format!("\x15{}\r", qmsg.msg));
                }
                // Claude/codex support bracketed paste: the frame goes as an
                // explicit paste (ESC[200~ … ESC[201~) and the \r right after
                // is a real Enter — deterministic, without the 300ms sleep or
                // racing the TUI's paste-detection window.
                "claude" | "codex" => {
                    let framed = frame_message(&qmsg);
                    write_to_pty(&state, &id, &format!("\x1b[200~{}\x1b[201~\r", framed));
                }
                // Unknown TUI (custom): keeps the two-phase burst — text
                // first, bare \r after the paste window.
                _ => {
                    let framed = frame_message(&qmsg);
                    write_to_pty(&state, &id, &framed);
                    let state_clone = Arc::clone(&state);
                    let id_clone = id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        write_to_pty(&state_clone, &id_clone, "\r");
                    });
                }
            }
            if let Some(tx) = qmsg.delivered_tx.take() {
                let _ = tx.send(());
            }
            let _ = app.emit("pty_status", PtyStatusEvent { id: id.clone(), status: RunStatus::Running.as_str() });
            let _ = app.emit("pty_queue", PtyQueueEvent { id, pending: items.len(), items });
        }
    }
}

/// `[narrater from X]: msg` or, with a msg_id (ask), `[narrater from X #id]: msg`.
fn frame_message(qmsg: &QueuedMsg) -> String {
    let id_part = qmsg.msg_id.as_deref().map(|i| format!(" #{}", i)).unwrap_or_default();
    format!("[narrater from {}{}]: {}", qmsg.from_label, id_part, qmsg.msg)
}

/// Deduplicates a label with a -2, -3… suffix (unless the owner is already `id`).
fn dedup_label(label_to_id: &HashMap<String, String>, id: &str, base: &str) -> String {
    let mut candidate = base.to_string();
    let mut n = 2;
    while label_to_id.get(&candidate).is_some_and(|owner| owner != id) {
        candidate = format!("{}-{}", base, n);
        n += 1;
    }
    candidate
}

pub fn write_to_pty(state_arc: &Arc<Mutex<PtyStateInner>>, id: &str, data: &str) -> bool {
    let mut inner = state_arc.lock().unwrap();
    if let Some(session) = inner.sessions.get_mut(id) {
        let _ = session.writer.write_all(data.as_bytes());
        let _ = session.writer.flush();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qmsg(from_label: &str, msg: &str, msg_id: Option<&str>) -> QueuedMsg {
        QueuedMsg {
            from_label: from_label.to_string(),
            from_id: None,
            msg: msg.to_string(),
            enqueued: Instant::now(),
            msg_id: msg_id.map(String::from),
            delivered_tx: None,
        }
    }

    #[test]
    fn frame_without_and_with_id() {
        assert_eq!(frame_message(&qmsg("planner", "hi", None)), "[narrater from planner]: hi");
        assert_eq!(
            frame_message(&qmsg("planner", "status?", Some("a3f2"))),
            "[narrater from planner #a3f2]: status?"
        );
    }

    #[test]
    fn dedup_label_suffixes_duplicates() {
        let mut map = HashMap::new();
        assert_eq!(dedup_label(&map, "t1", "claude"), "claude");
        map.insert("claude".to_string(), "t1".to_string());
        // The label's owner is this very id → keep it
        assert_eq!(dedup_label(&map, "t1", "claude"), "claude");
        // Another terminal with the same label → suffix
        assert_eq!(dedup_label(&map, "t2", "claude"), "claude-2");
        map.insert("claude-2".to_string(), "t2".to_string());
        assert_eq!(dedup_label(&map, "t3", "claude"), "claude-3");
    }
}
