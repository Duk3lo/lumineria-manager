mod agent_client;

use agent_client::AgentConnection;
use protocol::ClientRequest;
use tauri::State;
use tokio::sync::Mutex;

struct AppState {
    connection: Mutex<Option<AgentConnection>>,
}

#[tauri::command]
async fn connect_agent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let conn = agent_client::connect(app, url)
        .await
        .map_err(|e| e.to_string())?;
    *state.connection.lock().await = Some(conn);
    Ok(())
}

#[tauri::command]
async fn list_servers(state: State<'_, AppState>) -> Result<(), String> {
    send(&state, ClientRequest::ListServers).await
}

#[tauri::command]
async fn start_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::StartServer { id }).await
}

#[tauri::command]
async fn stop_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::StopServer { id }).await
}

#[tauri::command]
async fn restart_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::RestartServer { id }).await
}

#[tauri::command]
async fn sync_mods(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::SyncMods { id }).await
}

#[tauri::command]
async fn subscribe_logs(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::SubscribeLogs { id }).await
}

#[tauri::command]
async fn unsubscribe_logs(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::UnsubscribeLogs { id }).await
}

async fn send(state: &State<'_, AppState>, request: ClientRequest) -> Result<(), String> {
    let guard = state.connection.lock().await;
    match guard.as_ref() {
        Some(conn) => conn.send(request),
        None => Err("No conectado al agente todavía — llamá a connect_agent primero".into()),
    }
}

// ¡CAMBIO AQUÍ! En Tauri v2 esto debe llamarse `run` y ser público.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init()) // Plugin por defecto de Tauri v2
        .manage(AppState {
            connection: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            connect_agent,
            list_servers,
            start_server,
            stop_server,
            restart_server,
            sync_mods,
            subscribe_logs,
            unsubscribe_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error corriendo la app de Tauri");
}