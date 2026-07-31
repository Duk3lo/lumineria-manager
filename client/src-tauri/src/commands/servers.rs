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
pub async fn open_folder_in_os(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let command = "explorer";
    #[cfg(target_os = "linux")]
    let command = "xdg-open";
    #[cfg(target_os = "macos")]
    let command = "open";

    std::process::Command::new(command)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
