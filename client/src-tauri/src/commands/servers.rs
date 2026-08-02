use crate::core::state::AppState;
use protocol::{ClientRequest, ServerConfigParams};
use tauri::State;

pub async fn send(state: &State<'_, AppState>, request: ClientRequest) -> Result<(), String> {
    let guard = state.connection.lock().await;
    match guard.as_ref() {
        Some(conn) => conn.send(request),
        None => Err("No conectado al agente todavía — llamá a connect_agent primero".into()),
    }
}

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> Result<(), String> {
    send(&state, ClientRequest::ListServers).await
}

#[tauri::command]
pub async fn start_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::StartServer { id }).await
}

#[tauri::command]
pub async fn stop_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::StopServer { id }).await
}

#[tauri::command]
pub async fn restart_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::RestartServer { id }).await
}

#[tauri::command]
pub async fn sync_mods(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::SyncMods { id }).await
}

#[tauri::command]
pub async fn subscribe_logs(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::SubscribeLogs { id }).await
}

#[tauri::command]
pub async fn unsubscribe_logs(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::UnsubscribeLogs { id }).await
}

#[tauri::command]
pub async fn add_mod_packwiz(state: tauri::State<'_, AppState>, id: String, query: String) -> Result<(), String> {
    send(&state, ClientRequest::AddModPackwiz { id, query }).await
}

#[tauri::command]
pub async fn remove_mod_packwiz(state: tauri::State<'_, AppState>, id: String, query: String) -> Result<(), String> {
    send(&state, ClientRequest::RemoveModPackwiz { id, query }).await
}

#[tauri::command]
pub async fn publish_packwiz(state: tauri::State<'_, AppState>, id: String, pack_key: String, image: Option<protocol::PackwizImage>) -> Result<(), String> {
    send(&state, ClientRequest::PublishPackwiz { id, pack_key, image }).await
}

#[tauri::command]
pub async fn list_packwiz_mods(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::ListPackwizMods { id }).await
}

#[tauri::command]
pub async fn unpublish_packwiz(state: tauri::State<'_, AppState>, id: String, pack_key: String) -> Result<(), String> {
    send(&state, ClientRequest::UnpublishPackwiz { id, pack_key }).await
}

#[tauri::command]
pub async fn list_packwiz_files(state: tauri::State<'_, AppState>, id: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::ListPackwizFiles { id, scope }).await
}

#[tauri::command]
pub async fn read_packwiz_file(state: tauri::State<'_, AppState>, id: String, path: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::ReadFile { id, path, scope }).await
}

#[tauri::command]
pub async fn write_packwiz_file(state: tauri::State<'_, AppState>, id: String, path: String, content: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::WriteFile { id, path, content, scope }).await
}

#[tauri::command]
pub async fn delete_packwiz_file(state: tauri::State<'_, AppState>, id: String, path: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::DeleteFile { id, path, scope }).await
}

#[tauri::command]
pub async fn create_packwiz_directory(state: tauri::State<'_, AppState>, id: String, path: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::CreateDirectory { id, path, scope }).await
}

#[tauri::command]
pub async fn upload_mod_packwiz(state: tauri::State<'_, AppState>, id: String, filename: String, data_base64: String, folder: String, scope: protocol::FileScope) -> Result<(), String> {
    send(&state, ClientRequest::UploadModPackwiz { id, filename, data_base64, folder, scope }).await
}


#[tauri::command]
pub async fn list_velocity_plugins(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::ListVelocityPlugins { id }).await
}

#[tauri::command]
pub async fn add_velocity_plugin(
    state: tauri::State<'_, AppState>,
    id: String,
    source: protocol::PluginSource,
    value: String,
) -> Result<(), String> {
    send(&state, ClientRequest::AddVelocityPlugin { id, source, value }).await
}

#[tauri::command]
pub async fn remove_velocity_plugin(
    state: tauri::State<'_, AppState>,
    id: String,
    source: protocol::PluginSource,
    value: String,
) -> Result<(), String> {
    send(&state, ClientRequest::RemoveVelocityPlugin { id, source, value }).await
}

#[tauri::command]
pub async fn set_velocity_mc_version_hint(
    state: tauri::State<'_, AppState>,
    id: String,
    mc_version: Option<String>,
) -> Result<(), String> {
    send(&state, ClientRequest::SetVelocityMcVersionHint { id, mc_version }).await
}

#[tauri::command]
pub async fn update_server(
    state: State<'_, AppState>,
    id: String,
    loader_version: Option<String>,
    update_mods: bool,
    update_engine: bool,
    force: bool,
) -> Result<(), String> {
    send(&state, ClientRequest::UpdateServer { id, loader_version, update_mods, update_engine, force }).await
}


#[tauri::command]
pub async fn create_server(
    state: State<'_, AppState>,
    id: String,
    config: ServerConfigParams,
) -> Result<(), String> {
    send(&state, ClientRequest::CreateServer { id, config }).await
}

#[tauri::command]
pub async fn auto_update_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::AutoUpdateServer { id }).await
}

#[tauri::command]
pub async fn recreate_container(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::RecreateContainer { id }).await
}

#[tauri::command]
pub async fn delete_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::DeleteServer { id }).await
}

#[tauri::command]
pub async fn sync_pack_to_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::SyncPackToServer { id }).await
}

#[tauri::command]
pub async fn send_console_command(
    state: State<'_, AppState>,
    id: String,
    command: String,
) -> Result<(), String> {
    send(&state, ClientRequest::SendConsoleCommand { id, command }).await
}

#[tauri::command]
pub async fn open_folder_in_os(path: String) -> Result<(), String> {
    let canonical = std::fs::canonicalize(&path)
        .map_err(|_| "La carpeta indicada no existe.".to_string())?;
    if !canonical.is_dir() {
        return Err("La ruta indicada no es una carpeta.".to_string());
    }

    #[cfg(target_os = "windows")]
    let command = "explorer";
    #[cfg(target_os = "linux")]
    let command = "xdg-open";
    #[cfg(target_os = "macos")]
    let command = "open";

    std::process::Command::new(command)
        .arg(&canonical)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
