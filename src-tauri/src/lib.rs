mod asks;
mod bridge;
mod browser;
mod canvas_bridge;
mod chat;
mod composer;
pub mod environments;
pub mod cli_core;
mod floors;
mod fsops;
mod ipc;
mod names;
mod preferences;
mod pty;
mod roles;
mod routines;
mod scene;
pub mod transcript;
mod usage;
pub mod screen;
mod workspaces;

use pty::PtyState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pty_state = PtyState::default();
    let pty_state_for_ipc = pty_state.clone();
    let browser_state = browser::BrowserState::default();
    let browser_for_ipc = browser_state.clone();
    let routines_state = routines::RoutinesState::default();
    let routines_for_tasks = routines_state.clone();
    let chat_state = chat::ChatState::default();
    let chat_for_ipc = chat_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(pty_state)
        .manage(browser_state)
        .manage(routines_state)
        .manage(chat_state)
        .setup(move |app| {
            let state_arc = std::sync::Arc::clone(&pty_state_for_ipc.0);
            let app_handle = tauri::Manager::app_handle(app).clone();
            tauri::async_runtime::spawn(ipc::start_ipc_server(
                app_handle.clone(),
                std::sync::Arc::clone(&state_arc),
                browser_for_ipc,
                routines_for_tasks.clone(),
                chat_for_ipc,
            ));
            tauri::async_runtime::spawn(routines::start_scheduler(
                app_handle.clone(),
                routines_for_tasks,
                std::sync::Arc::clone(&state_arc),
            ));
            tauri::async_runtime::spawn(pty::start_status_monitor(app_handle, state_arc));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            usage::usage_snapshot,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_update_label,
            pty::connections_sync,
            pty::pty_statuses,
            pty::maestros_sync,
            routines::routines_load,
            routines::routines_list,
            routines::routines_wake,
            names::links_list,
            names::link_add,
            names::links_remove_node,
            names::links_scenes,
            preferences::preferences_get,
            composer::composer_send,
            composer::composer_attach,
            composer::composer_paste_image,
            scene::scene_backups,
            scene::scene_restore_backup,
            preferences::preferences_set,
            workspaces::workspace_set_environment,
            workspaces::environment_test,
            workspaces::environment_ssh_hosts,
            workspaces::environment_docker_containers,
            workspaces::environment_docker_contexts,
            workspaces::environment_sessions,
            workspaces::environment_end_sessions,
            routines::routines_sleep,
            routines::routine_ui_create,
            routines::routine_set_enabled,
            routines::routine_delete,
            routines::routine_run,
            workspaces::workspaces_layout,
            workspaces::workspace_set_dir,
            floors::floor_prepare,
            floors::floor_status,
            floors::floor_remove,
            floors::floor_land,
            floors::floor_delete,
            floors::floor_branches,
            floors::floor_pr_status,
            floors::floor_pr_merge,
            pty::pty_queue_cancel,
            pty::narrater_ledger,
            scene::load_scene,
            scene::save_scene,
            scene::list_scenes,
            scene::delete_scene,
            scene::rename_scene,
            scene::open_in_editor,
            fsops::fs_list_dir,
            fsops::fs_read_file_base64,
            fsops::pick_file,
            fsops::open_url,
            fsops::system_notify,
            fsops::open_path,
            chat::chat_send,
            chat::chat_history,
            chat::chat_threads,
            chat::chat_clear,
            canvas_bridge::canvas_respond,
            roles::load_roles,
            roles::save_roles,
            browser::portals_sync,
            browser::portal_open,
            browser::portal_attach,
            browser::portal_detach,
            browser::portal_navigate,
            browser::portal_history,
            browser::portal_input,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarraTer");
}
