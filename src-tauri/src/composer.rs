//! The prompt composer (Maestri's floating composer): the user writes a
//! prompt for a terminal outside it — multi-line, `@` references, pasted
//! screenshots — and it's typed in when the agent is idle, through the same
//! queue as messages between agents.

use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::pty::{enqueue_message, PtyState, PtyStateInner, QueuedMsg};
use crate::screen::TurnState;

/// Where a sent prompt landed in the terminal's queue.
#[derive(Serialize)]
pub struct Queued {
    /// Prompts ahead of it
    ahead: usize,
    /// The agent was busy: it's typed in once the agent is idle
    waiting: bool,
}

/// Queues `text` for the terminal.
#[tauri::command]
pub fn composer_send(terminal_id: String, text: String, app: AppHandle, state: State<'_, PtyState>) -> Result<Queued, String> {
    send(&state.0, &app, &terminal_id, text)
}

fn send(state: &Arc<Mutex<PtyStateInner>>, app: &AppHandle, terminal_id: &str, text: String) -> Result<Queued, String> {
    if text.trim().is_empty() {
        return Err("the prompt is empty".into());
    }
    let busy = match state.lock().unwrap().sessions.get(terminal_id) {
        Some(s) => s.turn != TurnState::Idle,
        None => return Err("that terminal isn't running — restart it first".into()),
    };
    let ahead = enqueue_message(state, app, terminal_id, QueuedMsg { from_label: "you".into(), from_id: None, msg: text, delivered_tx: None });
    Ok(Queued { ahead, waiting: busy || ahead > 0 })
}

/// Saves a pasted image where the terminal's agent can open it — here, or
/// in its remote workspace (`/tmp/narrater-drops`, like Maestri's drops) —
/// and returns that path.
#[tauri::command]
pub async fn composer_attach(
    terminal_id: String,
    data_base64: String,
    ext: String,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data_base64.as_bytes()).map_err(|e| e.to_string())?;
    save_drop(&state.0, &terminal_id, bytes, &ext).await
}

/// WebKitGTK hands the page an empty paste for images on the clipboard: the
/// composer asks here instead, and the image (if any) is saved like an
/// attachment. `None`: no image on the clipboard.
#[tauri::command]
pub async fn composer_paste_image(terminal_id: String, state: State<'_, PtyState>) -> Result<Option<String>, String> {
    match clipboard_image().await {
        Some((bytes, ext)) => save_drop(&state.0, &terminal_id, bytes, &ext).await.map(Some),
        None => Ok(None),
    }
}

/// The clipboard's image, through `wl-paste` (Wayland) or `xclip` (X11).
async fn clipboard_image() -> Option<(Vec<u8>, String)> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let tools: [(&str, &[&str], &[&str]); 2] = [
        ("wl-paste", &["--list-types"], &["--no-newline", "--type"]),
        ("xclip", &["-selection", "clipboard", "-o", "-t", "TARGETS"], &["-selection", "clipboard", "-o", "-t"]),
    ];
    let order: Vec<_> = if wayland { tools.iter().collect() } else { tools.iter().rev().collect() };
    for (bin, list_args, get_args) in order {
        let Some(types) = run_tool(bin, list_args).await else { continue };
        let types = String::from_utf8_lossy(&types);
        let types: Vec<&str> = types.lines().map(str::trim).filter(|t| t.starts_with("image/")).collect();
        let Some(mime) = types.iter().find(|t| **t == "image/png").or(types.first()) else { continue };
        let mut args = get_args.to_vec();
        args.push(mime);
        if let Some(bytes) = run_tool(bin, &args).await.filter(|b| !b.is_empty()) {
            return Some((bytes, mime.trim_start_matches("image/").to_string()));
        }
    }
    None
}

async fn run_tool(bin: &str, args: &[&str]) -> Option<Vec<u8>> {
    let child = tokio::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(3), child).await.ok()?.ok()?;
    out.status.success().then_some(out.stdout)
}

async fn save_drop(state: &Arc<Mutex<PtyStateInner>>, terminal_id: &str, bytes: Vec<u8>, ext: &str) -> Result<String, String> {
    let ext: String = ext.chars().filter(|c| c.is_ascii_alphanumeric()).take(5).collect();
    let ext = if ext.is_empty() { "png".to_string() } else { ext };
    let name = format!("{}.{}", &uuid::Uuid::new_v4().simple().to_string()[..12], ext);
    let remote = {
        let inner = state.lock().unwrap();
        inner.sessions.get(terminal_id).and_then(|s| s.remote.clone())
    };
    match remote {
        Some(env) => {
            let path = format!("/tmp/narrater-drops/{}", name);
            let script = format!(
                "mkdir -p /tmp/narrater-drops && base64 -d > {} <<'NARRATER_B64'\n{}\nNARRATER_B64\n",
                path,
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
            let place = env.describe();
            tauri::async_runtime::spawn_blocking(move || crate::environments::run(&env, &script))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("couldn't send the image to {}: {}", place, e))?;
            Ok(path)
        }
        None => {
            let dir = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater/drops");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(name);
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
    }
}
