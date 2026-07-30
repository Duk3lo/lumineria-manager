mod agent_client;

use agent_client::AgentConnection;
use protocol::ClientRequest;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LastConnection {
    mode: String, // "local" | "remote"
    folder: Option<String>,
    url: Option<String>,
}

fn last_connection_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("last_connection.json"))
}

#[tauri::command]
async fn load_last_connection(app: AppHandle) -> Result<Option<LastConnection>, String> {
    let path = last_connection_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_last_connection(
    app: AppHandle,
    mode: String,
    folder: Option<String>,
    url: Option<String>,
) -> Result<(), String> {
    let path = last_connection_path(&app)?;
    let record = LastConnection { mode, folder, url };
    let json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

struct AppState {
    connection: Mutex<Option<AgentConnection>>,
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder.map(|f| f.to_string()))
}

#[tauri::command]
async fn start_local_agent(app: AppHandle, root_path: String) -> Result<String, String> {
    if !std::path::Path::new(&root_path).is_dir() {
        return Err(format!(
            "La carpeta registrada ya no existe (fue eliminada o movida): {root_path}"
        ));
    }

    let port = "127.0.0.1:8756";

    let sidecar = app
        .shell()
        .sidecar("lumineria-agent")
        .map_err(|e| e.to_string())?
        .args(["serve", "--root", &root_path, "--bind", port]);

    let (mut rx, _child) = sidecar.spawn().map_err(|e| e.to_string())?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let tauri_plugin_shell::process::CommandEvent::Stdout(line) = event {
                println!("[Agente Local] {}", String::from_utf8_lossy(&line));
            }
        }
    });

    Ok(format!("ws://{port}/ws"))
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

#[tauri::command]
async fn fetch_paper_project_versions(project: String) -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let url = format!("https://fill.papermc.io/v3/projects/{}", project);
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "LumineriaManager/2.0 (contacto: admin@lumineria.local)",
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} al consultar {}",
            response.status(),
            project
        ));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let versions_obj = json["versions"]
        .as_object()
        .ok_or_else(|| "Formato inesperado: falta 'versions'".to_string())?;

    let versions: Vec<String> = versions_obj
        .values()
        .filter_map(|v| v.as_array())
        .flatten()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    Ok(versions)
}

#[tauri::command]
async fn fetch_neoforge_versions() -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let versions = json["versions"]
        .as_array()
        .ok_or_else(|| "Formato inesperado".to_string())?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Ok(versions)
}

#[tauri::command]
async fn fetch_forge_versions() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json)
}

#[tauri::command]
async fn create_server(
    state: State<'_, AppState>,
    id: String,
    config: protocol::ServerConfigParams,
) -> Result<(), String> {
    send(&state, ClientRequest::CreateServer { id, config }).await
}

#[tauri::command]
async fn auto_update_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::AutoUpdateServer { id }).await
}

#[tauri::command]
async fn recreate_container(state: State<'_, AppState>, id: String) -> Result<(), String> {
    send(&state, ClientRequest::RecreateContainer { id }).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            connection: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            start_local_agent,
            connect_agent,
            list_servers,
            start_server,
            stop_server,
            restart_server,
            sync_mods,
            subscribe_logs,
            unsubscribe_logs,
            fetch_neoforge_versions,
            fetch_forge_versions,
            fetch_paper_project_versions,
            create_server,
            auto_update_server,
            load_last_connection,
            save_last_connection,
            recreate_container,
        ])
        .run(tauri::generate_context!())
        .expect("error corriendo la app de Tauri");
}
