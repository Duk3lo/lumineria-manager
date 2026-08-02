mod commands;
mod core;

use core::state::AppState;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            connection: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            // --- Connection / Local Agent ---
            commands::connection::pick_folder,
            commands::connection::start_local_agent,
            commands::connection::connect_agent,
            commands::connection::load_last_connection,
            commands::connection::save_last_connection,
            // --- Servers / Docker ---
            commands::servers::list_servers,
            commands::servers::start_server,
            commands::servers::stop_server,
            commands::servers::restart_server,
            commands::servers::delete_server,
            commands::servers::recreate_container,
            commands::servers::create_server,
            commands::servers::auto_update_server,
            commands::servers::sync_mods,
            commands::servers::open_folder_in_os,
            commands::servers::subscribe_logs,
            commands::servers::unsubscribe_logs,
            commands::servers::add_mod_packwiz,
            commands::servers::remove_mod_packwiz,
            commands::servers::upload_mod_packwiz,
            commands::servers::publish_packwiz,
            commands::servers::unpublish_packwiz,
            commands::servers::list_packwiz_mods,
            commands::connection::load_publish_config,
            commands::connection::save_publish_config,
            commands::servers::send_console_command,
            commands::servers::list_packwiz_files,
            commands::servers::read_packwiz_file,
            commands::servers::write_packwiz_file,
            commands::servers::delete_packwiz_file,
            commands::servers::create_packwiz_directory,
            commands::servers::update_server,
            commands::servers::sync_pack_to_server,
            commands::servers::list_velocity_plugins,
            commands::servers::add_velocity_plugin,
            commands::servers::remove_velocity_plugin,
            commands::servers::set_velocity_mc_version_hint,
            // --- Versions ---
            commands::versions::fetch_neoforge_versions,
            commands::versions::fetch_forge_versions,
            commands::versions::fetch_paper_project_versions,
        ])
        .run(tauri::generate_context!())
        .expect("error corriendo la app de Tauri");
}
