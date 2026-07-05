use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tauri::{AppHandle, Emitter, State};
use serde::Serialize;

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

#[derive(Default)]
pub struct PtyState(pub Mutex<HashMap<String, PtySession>>);

#[tauri::command]
pub fn pty_spawn(
    id: String,
    command: String,
    cols: u16,
    rows: u16,
    app_handle: AppHandle,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new(&command);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    // Inherit the user's home and path
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }

    let _child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| e.to_string())?;

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let id_clone = id.clone();
    let app_clone = app_handle.clone();

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = app_clone.emit("pty_exit", PtyExit { id: id_clone, code: 0 });
                    break;
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app_clone.emit("pty_output", PtyOutput { id: id_clone.clone(), data });
                }
            }
        }
    });

    state.0.lock().unwrap().insert(id, PtySession { writer: Box::new(writer) });
    Ok(())
}

#[tauri::command]
pub fn pty_write(id: String, data: String, state: State<'_, PtyState>) -> Result<(), String> {
    let mut sessions = state.0.lock().unwrap();
    if let Some(session) = sessions.get_mut(&id) {
        session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        session.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(_id: String, _cols: u16, _rows: u16, _state: State<'_, PtyState>) -> Result<(), String> {
    // Resize support requires storing the master handle; add in next iteration
    Ok(())
}

#[tauri::command]
pub fn pty_kill(id: String, state: State<'_, PtyState>) -> Result<(), String> {
    state.0.lock().unwrap().remove(&id);
    Ok(())
}
