use crate::pty::PtyState;
use tauri::State;

/// Resolve de uma requisição de canvas em andamento (evento `canvas_request`):
/// o frontend aplica a ação no store e devolve o resultado por aqui, acordando
/// o handler do IPC que está aguardando no socket.
#[tauri::command]
pub fn canvas_respond(state: State<PtyState>, req_id: String, result: String) -> Result<(), String> {
    let tx = state.0.lock().unwrap().canvas_waiters.remove(&req_id);
    match tx {
        Some(tx) => tx
            .send(result)
            .map_err(|_| "requisição de canvas já expirou".to_string()),
        None => Err(format!("req_id de canvas desconhecido: {}", req_id)),
    }
}
