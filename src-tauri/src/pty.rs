use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tauri::{AppHandle, Emitter, State};
use serde::Serialize;
use tokio::sync::mpsc;

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

pub struct PtySession {
    writer: Box<dyn Write + Send>,
}

pub struct PtyStateInner {
    pub sessions: HashMap<String, PtySession>,
    pub labels: HashMap<String, String>,
    pub label_to_id: HashMap<String, String>,
    pub response_listeners: HashMap<String, mpsc::Sender<String>>,
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
        })))
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
) -> Result<(), String> {
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

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    {
        let mut inner = state.0.lock().unwrap();
        if let Some(lbl) = &label {
            inner.labels.insert(id.clone(), lbl.clone());
            inner.label_to_id.insert(lbl.clone(), id.clone());
        }
        inner.sessions.insert(id.clone(), PtySession { writer: Box::new(writer) });
    }

    let id_clone = id.clone();
    let app_clone = app_handle.clone();
    let state_arc = Arc::clone(&state.0);

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    state_arc.lock().unwrap().response_listeners.remove(&id_clone);
                    let _ = app_clone.emit("pty_exit", PtyExit { id: id_clone, code: 0 });
                    break;
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app_clone.emit("pty_output", PtyOutput { id: id_clone.clone(), data: data.clone() });

                    let sender = state_arc.lock().unwrap()
                        .response_listeners.get(&id_clone).cloned();
                    if let Some(tx) = sender {
                        let _ = tx.try_send(data);
                    }
                }
            }
        }
    });

    Ok(())
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
pub fn pty_resize(_id: String, _cols: u16, _rows: u16, _state: State<'_, PtyState>) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn pty_kill(id: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    inner.sessions.remove(&id);
    if let Some(label) = inner.labels.remove(&id) {
        inner.label_to_id.remove(&label);
    }
    Ok(())
}

#[tauri::command]
pub fn pty_update_label(id: String, label: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut inner = state.0.lock().unwrap();
    if let Some(old_label) = inner.labels.get(&id).cloned() {
        inner.label_to_id.remove(&old_label);
    }
    inner.labels.insert(id.clone(), label.clone());
    inner.label_to_id.insert(label, id);
    Ok(())
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
