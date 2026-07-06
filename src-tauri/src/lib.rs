mod canvas_bridge;
mod fsops;
mod historia;
mod ipc;
mod pty;
mod roles;

use pty::PtyState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pty_state = PtyState::default();
    let pty_state_for_ipc = pty_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(pty_state)
        .setup(move |app| {
            let state_arc = std::sync::Arc::clone(&pty_state_for_ipc.0);
            let app_handle = tauri::Manager::app_handle(app).clone();
            tauri::async_runtime::spawn(ipc::start_ipc_server(
                app_handle.clone(),
                std::sync::Arc::clone(&state_arc),
            ));
            tauri::async_runtime::spawn(pty::start_status_monitor(app_handle, state_arc));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_update_label,
            pty::connections_sync,
            pty::pty_notify,
            pty::pty_queue_cancel,
            pty::narrater_ledger,
            historia::load_historia,
            historia::save_historia,
            historia::list_historias,
            historia::delete_historia,
            historia::rename_historia,
            historia::open_in_editor,
            fsops::fs_list_dir,
            fsops::fs_read_file_base64,
            fsops::pick_file,
            fsops::open_url,
            canvas_bridge::canvas_respond,
            roles::load_roles,
            roles::save_roles,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarraTer");
}
