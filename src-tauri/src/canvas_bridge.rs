use crate::pty::PtyState;
use tauri::State;

/// Resolves an in-flight canvas request (`canvas_request` event): the
/// frontend applies the action on the store and returns the result here,
/// waking the IPC handler waiting on the socket.
#[tauri::command]
pub fn canvas_respond(state: State<PtyState>, req_id: String, result: String) -> Result<(), String> {
    let tx = state.0.lock().unwrap().canvas_waiters.remove(&req_id);
    match tx {
        Some(tx) => tx
            .send(result)
            .map_err(|_| "canvas request already expired".to_string()),
        None => Err(format!("unknown canvas req_id: {}", req_id)),
    }
}
