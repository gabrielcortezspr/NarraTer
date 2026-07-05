use std::collections::{HashMap, HashSet};
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
}

pub struct PtyStateInner {
    pub sessions: HashMap<String, PtySession>,
    pub labels: HashMap<String, String>,
    pub label_to_id: HashMap<String, String>,
    pub response_listeners: HashMap<String, mpsc::Sender<String>>,
    /// Directed agent-pipe routes (source node id → target node id), mirrored
    /// from the canvas edges. Communication is only allowed along these.
    pub connections: HashSet<(String, String)>,
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
            connections: HashSet::new(),
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
    cols: u16,
    rows: u16,
    label: Option<String>,
    app_handle: AppHandle,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new(&command);
    cmd.env("TERM", "xterm-256color");
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

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let effective_label = {
        let mut inner = state.0.lock().unwrap();
        // Labels address agents in `narrater send`; disambiguate duplicates so
        // two "claude" terminals never clobber each other's route.
        let base = label.unwrap_or_else(|| id.clone());
        let mut candidate = base.clone();
        let mut n = 2;
        while inner.label_to_id.get(&candidate).is_some_and(|owner| owner != &id) {
            candidate = format!("{}-{}", base, n);
            n += 1;
        }
        inner.labels.insert(id.clone(), candidate.clone());
        inner.label_to_id.insert(candidate.clone(), id.clone());
        inner.sessions.insert(id.clone(), PtySession {
            writer: Box::new(writer),
            master: pair.master,
            child,
            last_output: Instant::now(),
            status: RunStatus::Running,
        });
        candidate
    };

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
                        inner.response_listeners.remove(&id_clone);
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

                    let (sender, became_running) = {
                        let mut inner = state_arc.lock().unwrap();
                        let mut became_running = false;
                        if let Some(session) = inner.sessions.get_mut(&id_clone) {
                            session.last_output = Instant::now();
                            if session.status == RunStatus::Idle {
                                session.status = RunStatus::Running;
                                became_running = true;
                            }
                        }
                        (inner.response_listeners.get(&id_clone).cloned(), became_running)
                    };
                    if became_running {
                        let _ = app_clone.emit("pty_status", PtyStatusEvent {
                            id: id_clone.clone(),
                            status: RunStatus::Running.as_str(),
                        });
                    }
                    if let Some(tx) = sender {
                        let _ = tx.try_send(data);
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

/// Background task: flips Running sessions to Idle after IDLE_THRESHOLD of
/// output silence and notifies the frontend on each transition.
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
