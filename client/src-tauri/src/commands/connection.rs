use crate::core::agent_client;
use crate::core::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LastConnection {
    pub mode: String,
    pub folder: Option<String>,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishConfig {
    pub ssh_host: Option<String>,
    pub remote_base: String,
    pub domain: String,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            ssh_host: None,
            remote_base: "~/lumineria".to_string(),
            domain: "http://localhost".to_string(),
        }
    }
}
fn publish_config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("publish_config.json"))
}

#[tauri::command]
pub async fn load_publish_config(app: AppHandle) -> Result<PublishConfig, String> {
    let path = publish_config_path(&app)?;
    if !path.exists() {
        return Ok(PublishConfig::default());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_publish_config(
    app: AppHandle,
    ssh_host: Option<String>,
    remote_base: String,
    domain: String,
) -> Result<(), String> {
    let path = publish_config_path(&app)?;
    let cfg = PublishConfig {
        ssh_host,
        remote_base,
        domain,
    };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

fn last_connection_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("last_connection.json"))
}

#[tauri::command]
pub async fn load_last_connection(app: AppHandle) -> Result<Option<LastConnection>, String> {
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
pub async fn save_last_connection(
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

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder.map(|f| f.to_string()))
}

fn agent_pid_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("local_agent.pid"))
}

#[tauri::command]
pub async fn start_local_agent(app: AppHandle, root_path: String) -> Result<String, String> {
    if !std::path::Path::new(&root_path).is_dir() {
        return Err(format!("La carpeta registrada ya no existe: {root_path}"));
    }

    let pid_path = agent_pid_path(&app)?;
    if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status();
        }
    }

    let port = "127.0.0.1:8756";
    let publish_cfg = load_publish_config(app.clone()).await?;
    let token = generate_token();

    let mut args = vec![
        "serve".to_string(), "--root".to_string(), root_path.clone(),
        "--bind".to_string(), port.to_string(),
        "--token".to_string(), token.clone(),   // 👈 nuevo
        "--vps-remote-base".to_string(), publish_cfg.remote_base.clone(),
        "--domain".to_string(), publish_cfg.domain.clone(),
    ];
    if let Some(host) = &publish_cfg.ssh_host {
        args.push("--vps-ssh-host".to_string());
        args.push(host.clone());
    }

    let sidecar = app
        .shell()
        .sidecar("lumineria-agent")
        .map_err(|e| e.to_string())?
        .args(args);
    let (mut rx, child) = sidecar.spawn().map_err(|e| e.to_string())?;
    std::fs::write(&pid_path, child.pid().to_string()).map_err(|e| e.to_string())?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                tauri_plugin_shell::process::CommandEvent::Stdout(l) => {
                    println!("[Agente Local] {}", String::from_utf8_lossy(&l))
                }
                tauri_plugin_shell::process::CommandEvent::Stderr(l) => {
                    eprintln!("[Agente Local ERROR] {}", String::from_utf8_lossy(&l))
                }
                _ => {}
            }
        }
    });

    Ok(format!("ws://{port}/ws?token={token}"))
}

fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[tauri::command]
pub async fn connect_agent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let mut last_err = String::new();
    for _ in 0..15 {
        match agent_client::connect(app.clone(), url.clone()).await {
            Ok(conn) => {
                *state.connection.lock().await = Some(conn);
                return Ok(());
            }
            Err(e) => {
                last_err = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
    Err(format!("No pude conectar tras varios intentos: {last_err}"))
}
