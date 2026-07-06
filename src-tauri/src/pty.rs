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

#[derive(Serialize, Clone)]
pub struct PtyQueueEvent {
    pub id: String,
    pub pending: usize,
}

/// Message waiting to be injected into a terminal's stdin.
pub struct QueuedMsg {
    pub from_label: String,
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
    /// Per-terminal inbound message queue, drained on idle by the monitor.
    pub inbox: HashMap<String, VecDeque<QueuedMsg>>,
    /// In-flight canvas requests (MCP → frontend), keyed by request id; the
    /// frontend resolves them via the canvas_respond command.
    pub canvas_waiters: HashMap<String, tokio::sync::oneshot::Sender<String>>,
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
            inbox: HashMap::new(),
            canvas_waiters: HashMap::new(),
        })))
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
            let program = parts.next().ok_or("comando vazio")?;
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
        let mut candidate = base.clone();
        let mut n = 2;
        while inner.label_to_id.get(&candidate).is_some_and(|owner| owner != &id) {
            candidate = format!("{}-{}", base, n);
            n += 1;
        }
        inner.labels.insert(id.clone(), candidate.clone());
        inner.label_to_id.insert(candidate.clone(), id.clone());
        candidate
    };
    cmd.env("NARRATER_ID", &id);
    cmd.env("NARRATER_LABEL", &effective_label);

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
            agent_type: agent_type.unwrap_or_else(|| "shell".to_string()),
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
        return Err(format!("Label '{}' já está em uso por outro terminal", label));
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
/// auto-submitted as `[narrater de sistema]: <text>`. Never use on shell
/// targets: shell delivery executes the text as a command.
#[tauri::command]
pub fn pty_notify(id: String, text: String, app_handle: AppHandle, state: State<'_, PtyState>) -> Result<(), String> {
    enqueue_message(&state.0, &app_handle, &id, QueuedMsg {
        from_label: "sistema".to_string(),
        msg: text,
        enqueued: Instant::now(),
        msg_id: None,
        delivered_tx: None,
    });
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
    let pending = {
        let mut inner = state_arc.lock().unwrap();
        let queue = inner.inbox.entry(target_id.to_string()).or_default();
        queue.push_back(msg);
        queue.len()
    };
    let _ = app.emit("pty_queue", PtyQueueEvent { id: target_id.to_string(), pending });
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
                .filter(|(_, s)| s.status == RunStatus::Running && s.last_output.elapsed() >= IDLE_THRESHOLD)
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
        let deliveries: Vec<(String, QueuedMsg, usize, bool)> = {
            let mut inner = state.lock().unwrap();
            let ids: Vec<String> = inner.inbox.keys().cloned().collect();
            let mut out = Vec::new();
            for id in ids {
                let Some(session) = inner.sessions.get(&id) else {
                    inner.inbox.remove(&id);
                    continue;
                };
                let ready = match session.status {
                    RunStatus::Idle => true,
                    RunStatus::Running => inner
                        .inbox
                        .get(&id)
                        .and_then(|q| q.front())
                        .is_some_and(|m| m.enqueued.elapsed() >= MAX_QUEUE_WAIT),
                };
                if !ready {
                    continue;
                }
                if let Some(queue) = inner.inbox.get_mut(&id) {
                    if let Some(msg) = queue.pop_front() {
                        let pending = queue.len();
                        if queue.is_empty() {
                            inner.inbox.remove(&id);
                        }
                        // Mark Running right away so the whole queue isn't
                        // flushed before the injected command produces output
                        let mut is_shell = true;
                        if let Some(s) = inner.sessions.get_mut(&id) {
                            s.status = RunStatus::Running;
                            s.last_output = Instant::now();
                            is_shell = s.agent_type == "shell";
                        }
                        out.push((id.clone(), msg, pending, is_shell));
                    }
                }
            }
            out
        };

        for (id, mut qmsg, pending, is_shell) in deliveries {
            // \r submits the line (terminals send Enter as carriage return —
            // \n is not enough for raw-mode TUIs). Shells get the bare command
            // (the sender frame would break it) preceded by kill-line (^U) to
            // clear any half-typed input; AI agents get the sender frame.
            if is_shell {
                write_to_pty(&state, &id, &format!("\x15{}\r", qmsg.msg));
            } else {
                // TUIs like claude code treat a fast text+\r burst as a paste,
                // turning the \r into a literal newline in the composer. Send
                // the text first and the \r alone after the paste-detection
                // window so it registers as a real Enter.
                let id_part = qmsg.msg_id.as_deref().map(|i| format!(" #{}", i)).unwrap_or_default();
                let framed = format!("[narrater de {}{}]: {}", qmsg.from_label, id_part, qmsg.msg);
                write_to_pty(&state, &id, &framed);
                let state_clone = Arc::clone(&state);
                let id_clone = id.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    write_to_pty(&state_clone, &id_clone, "\r");
                });
            }
            if let Some(tx) = qmsg.delivered_tx.take() {
                let _ = tx.send(());
            }
            let _ = app.emit("pty_status", PtyStatusEvent { id: id.clone(), status: RunStatus::Running.as_str() });
            let _ = app.emit("pty_queue", PtyQueueEvent { id, pending });
        }
    }
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
