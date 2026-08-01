use crate::docker::{discovery, podman};
use crate::installer::{config, installer};
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientRequest, ServerEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct AppState {
    root: Arc<PathBuf>,
    publish_target: Arc<crate::publisher::PublishTarget>,
    domain: Arc<String>,
}

pub async fn serve(
    root: PathBuf,
    bind: String,
    publish_target: crate::publisher::PublishTarget,
    domain: String,
) -> Result<()> {
    let state = AppState {
        root: Arc::new(root),
        publish_target: Arc::new(publish_target),
        domain: Arc::new(domain),
    };
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);
    tracing::info!("Agente escuchando en {bind} (recordá: solo detrás de un túnel SSH)");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerEvent>();

    let writer_task = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let Ok(json) = serde_json::to_string(&event) else {
                continue;
            };
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    let mut log_tasks: HashMap<String, JoinHandle<()>> = HashMap::new();

    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(text) = msg else { continue };
        let request: ClientRequest = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(ServerEvent::Error {
                    message: format!("mensaje inválido: {e}"),
                });
                continue;
            }
        };
        handle_request(request, &state, &tx, &mut log_tasks).await;
    }

    for (_, handle) in log_tasks {
        handle.abort();
    }
    writer_task.abort();
}

async fn ensure_packwiz_initialized(
    dest_dir: &std::path::Path,
    pack_dir: &std::path::Path,
    packwiz_bin: &std::path::Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> anyhow::Result<()> {
    if pack_dir.join("pack.toml").exists() {
        return Ok(());
    }
    tokio::fs::create_dir_all(pack_dir).await?;
    let _ = tx.send(ServerEvent::PackwizLog {
        id: id.to_string(),
        line: "⚠️ No se encontró un modpack. Auto-inicializando...".into(),
    });
    let env_path = dest_dir.join("server.env");
    let env_data = fs::read_to_string(&env_path).await.unwrap_or_default();
    let mut mc_version = "1.21.1".to_string();
    let mut server_type = "fabric".to_string();
    for line in env_data.lines() {
        if line.starts_with("MC_VERSION=") {
            mc_version = line
                .replace("MC_VERSION=", "")
                .replace('"', "")
                .trim()
                .to_string();
        }
        if line.starts_with("SERVER_TYPE=") {
            server_type = line
                .replace("SERVER_TYPE=", "")
                .replace('"', "")
                .trim()
                .to_string();
        }
    }
    let loader = match server_type.as_str() {
        "paper" | "velocity" => "none",
        other => other,
    };
    let status = tokio::process::Command::new(packwiz_bin)
        .args([
            "init",
            "--name",
            id,
            "--author",
            "Lumineria",
            "--mc-version",
            &mc_version,
            "--modloader",
            loader,
            "-y",
        ])
        .current_dir(pack_dir)
        .status()
        .await?;
    if !status.success() {
        anyhow::bail!("No pude inicializar packwiz para '{}'", id);
    }
    Ok(())
}

async fn handle_request(
    request: ClientRequest,
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    log_tasks: &mut HashMap<String, JoinHandle<()>>,
) {
    match request {
        ClientRequest::ListServers => match discovery::discover(&state.root) {
            Ok(list) => {
                let _ = tx.send(ServerEvent::Servers { servers: list });
            }
            Err(e) => {
                let _ = tx.send(ServerEvent::Error {
                    message: format!("no pude listar servidores: {e}"),
                });
            }
        },
        ClientRequest::StartServer { id } => {
            run_action(tx, &id, podman::container_action("start", &id).await)
        }
        ClientRequest::StopServer { id } => {
            run_action(tx, &id, podman::container_action("stop", &id).await)
        }
        ClientRequest::RestartServer { id } => {
            run_action(tx, &id, podman::container_action("restart", &id).await)
        }
        ClientRequest::SyncMods { id } => run_action(tx, &id, podman::sync_mods_now(&id).await),
        ClientRequest::StartStack => run_action(
            tx,
            "stack",
            podman::run_stack_script(&state.root, "start-podman.sh").await,
        ),
        ClientRequest::StopStack => run_action(
            tx,
            "stack",
            podman::run_stack_script(&state.root, "stop-podman.sh").await,
        ),
        ClientRequest::RestartStack => run_action(
            tx,
            "stack",
            podman::run_stack_script(&state.root, "restart-podman.sh").await,
        ),
        ClientRequest::SubscribeLogs { id } => {
            if let Some(handle) = log_tasks.get(&id) {
                if !handle.is_finished() {
                    return;
                }
            }
            let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
            let container_id = id.clone();
            tokio::spawn(async move {
                if let Err(e) = podman::stream_logs(container_id, line_tx).await {
                    tracing::warn!("stream_logs terminó con error: {e}");
                }
            });
            let event_tx = tx.clone();
            let event_id = id.clone();
            let handle = tokio::spawn(async move {
                while let Some(line) = line_rx.recv().await {
                    if event_tx
                        .send(ServerEvent::LogLine {
                            id: event_id.clone(),
                            line,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
            log_tasks.insert(id, handle);
        }
        ClientRequest::UnsubscribeLogs { id } => {
            if let Some(handle) = log_tasks.remove(&id) {
                handle.abort();
            }
        }
        ClientRequest::CreateServer { id, config: cfg } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id);
                if let Err(e) = fs::create_dir_all(&dest_dir).await {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: format!("No se pudo crear directorio: {e}"),
                    });
                    return;
                }
                let client = reqwest::Client::new();
                let _ = tx_clone.send(ServerEvent::InstallProgress {
                    id: id.clone(),
                    step: "Iniciando instalación".into(),
                    percentage: 5,
                });
                let rcon_creds = config::rcon_credentials_for(&cfg);
                if config::write_server_env(&dest_dir, &cfg, &rcon_creds)
                    .await
                    .is_err()
                    || config::write_server_properties(&dest_dir, &cfg, &rcon_creds)
                        .await
                        .is_err()
                {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: "Error al escribir configuraciones locales.".into(),
                    });
                    return;
                }
                let _ = tokio::fs::write(dest_dir.join("eula.txt"), "eula=true\n").await;
                let result = match cfg.server_type.as_str() {
                    "paper" | "velocity" | "folia" => installer::install_papermc(
                        &client,
                        &cfg.server_type,
                        &cfg.mc_version,
                        &dest_dir,
                        &id,
                        &tx_clone,
                        &cfg.min_ram,
                        &cfg.max_ram,
                    )
                    .await
                    .map(|_| ()),
                    "fabric" => {
                        let loader = cfg.loader_version.clone().unwrap_or_default();
                        installer::install_fabric(
                            &client,
                            &cfg.mc_version,
                            &loader,
                            &dest_dir,
                            &id,
                            &tx_clone,
                            &cfg.min_ram,
                            &cfg.max_ram,
                        )
                        .await
                    }
                    "neoforge" => {
                        let loader = cfg.loader_version.clone().unwrap_or_default();
                        let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", loader);
                        installer::install_mod_installer(
                            &client,
                            &url,
                            &format!("neoforge-{}-installer.jar", loader),
                            &dest_dir,
                            &id,
                            &tx_clone,
                            &cfg.min_ram,
                            &cfg.max_ram,
                        )
                        .await
                    }
                    "forge" => {
                        let loader = cfg.loader_version.clone().unwrap_or_default();
                        let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar", loader);
                        installer::install_mod_installer(
                            &client,
                            &url,
                            &format!("forge-{}-installer.jar", loader),
                            &dest_dir,
                            &id,
                            &tx_clone,
                            &cfg.min_ram,
                            &cfg.max_ram,
                        )
                        .await
                    }
                    _ => Ok(()),
                };
                if let Err(e) = result {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: format!("Error de instalación: {e}"),
                    });
                    return;
                }
                let _ = tx_clone.send(ServerEvent::InstallProgress {
                    id: id.clone(),
                    step: "Creando contenedor".into(),
                    percentage: 90,
                });
                let image = podman::java_image_for(&cfg.server_type, &cfg.mc_version);
                match podman::create_container(&id, &dest_dir, image).await {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: Some("Instalado exitosamente".into()),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("Error: {e}"),
                        });
                    }
                }
            });
        }
        ClientRequest::AutoUpdateServer { id } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();

            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id);
                let env_path = dest_dir.join("server.env");

                let env_data = match fs::read_to_string(&env_path).await {
                    Ok(data) => data,
                    Err(_) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: "Instancia no válida para auto-actualización".into(),
                        });
                        return;
                    }
                };

                let mut mc_version = "latest".to_string();
                let mut server_type = "paper".to_string();
                let mut min_ram = "1G".to_string();
                let mut max_ram = "4G".to_string();

                for line in env_data.lines() {
                    if line.starts_with("MC_VERSION=") {
                        mc_version = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("latest")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("SERVER_TYPE=") {
                        server_type = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("paper")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("MIN_RAM=") {
                        min_ram = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("1G")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("MAX_RAM=") {
                        max_ram = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("4G")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                }

                let client = reqwest::Client::new();
                let _ = tx_clone.send(ServerEvent::InstallProgress {
                    id: id.clone(),
                    step: "Buscando actualizaciones estables...".into(),
                    percentage: 20,
                });

                let result = match server_type.as_str() {
                    "paper" | "velocity" | "folia" => installer::install_papermc(
                        &client,
                        &server_type,
                        &mc_version,
                        &dest_dir,
                        &id,
                        &tx_clone,
                        &min_ram,
                        &max_ram,
                    )
                    .await
                    .map(|_| ()),
                    _ => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message:
                                "Auto-actualización solo disponible en motores Vanilla/PaperMC"
                                    .into(),
                        });
                        return;
                    }
                };

                match result {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: Some("Motor actualizado. Se usará al reiniciar.".into()),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("Error al actualizar: {e}"),
                        });
                    }
                }
            });
        }

        ClientRequest::RecreateContainer { id } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id);
                let env_path = dest_dir.join("server.env");

                let env_data = match fs::read_to_string(&env_path).await {
                    Ok(d) => d,
                    Err(_) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: "No encontré la configuración.".into(),
                        });
                        return;
                    }
                };

                let mut server_type = "paper".to_string();
                let mut mc_version = "latest".to_string();

                for line in env_data.lines() {
                    if line.starts_with("SERVER_TYPE=") {
                        server_type = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("paper")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("MC_VERSION=") {
                        mc_version = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("latest")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                }

                if !dest_dir.join("start.sh").exists() {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: "Falta start.sh — reinstala el motor antes de recrear.".into(),
                    });
                    return;
                }

                let image = podman::java_image_for(&server_type, &mc_version);
                match podman::create_container(&id, &dest_dir, image).await {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: Some("Contenedor recreado".into()),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("Error: {e}"),
                        });
                    }
                }
            });
        }

        ClientRequest::DeleteServer { id } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                if let Err(e) = podman::delete_container(&id).await {
                    tracing::warn!("Problema al borrar contenedor {id}: {e}");
                }

                let dest_dir = root_clone.join(&id);
                if dest_dir.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&dest_dir).await {
                        tracing::warn!(
                            "Fallo borrado normal de {id}: {e}. Intentando con podman unshare..."
                        );
                        let unshare_status = tokio::process::Command::new("podman")
                            .args(["unshare", "rm", "-rf", dest_dir.to_string_lossy().as_ref()])
                            .status()
                            .await;

                        if unshare_status.is_err() || !unshare_status.unwrap().success() {
                            let _ = tx_clone.send(ServerEvent::Error {
                                message: format!("El contenedor se borró, pero no la carpeta (requiere sudo): {e}"),
                            });
                            return;
                        }
                    }
                }

                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: Some("Servidor y archivos eliminados exitosamente".into()),
                });
            });
        }

        ClientRequest::AddModPackwiz { id, query } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id);
                let pack_dir = dest_dir.join("packwiz");
                let packwiz_bin = crate::system::deps::find_in_path("packwiz")
                    .unwrap_or_else(|| std::path::PathBuf::from("packwiz"));

                if let Err(e) =
                    ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx_clone)
                        .await
                {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ {}", e),
                    });
                    return;
                }

                let source = if query.contains("curseforge.com") {
                    "cf"
                } else {
                    "modrinth"
                };
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("> Añadiendo mod '{}'...", query),
                });

                let output = tokio::process::Command::new(&packwiz_bin)
                    .arg(source)
                    .arg("add")
                    .arg(&query)
                    .current_dir(&pack_dir)
                    .output()
                    .await;

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        if !stdout.is_empty() {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: stdout.trim().to_string(),
                            });
                        }
                        if !stderr.is_empty() {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: stderr.trim().to_string(),
                            });
                        }

                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: None,
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ Error: {}", e),
                        });
                    }
                }
            });
        }

        ClientRequest::RemoveModPackwiz { id, query } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id).join("packwiz");
                let packwiz_bin = crate::system::deps::find_in_path("packwiz")
                    .unwrap_or_else(|| std::path::PathBuf::from("packwiz"));

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("> Eliminando mod '{}'...", query),
                });
                let output = tokio::process::Command::new(&packwiz_bin)
                    .arg("remove")
                    .arg(&query)
                    .current_dir(&dest_dir)
                    .output()
                    .await;

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        if !stdout.is_empty() {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: stdout.trim().to_string(),
                            });
                        }
                        if !stderr.is_empty() {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: stderr.trim().to_string(),
                            });
                        }
                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: None,
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ Error: {}", e),
                        });
                    }
                }
            });
        }

        ClientRequest::UploadModPackwiz {
            id,
            filename,
            data_base64,
            folder,
        } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let dest_dir = root_clone.join(&id).join("packwiz");

                let is_root = folder.is_empty() || folder == "." || folder == "root";

                let target_dir = if is_root {
                    dest_dir.clone()
                } else {
                    dest_dir.join(&folder)
                };

                let _ = tokio::fs::create_dir_all(&target_dir).await;

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: if is_root {
                        format!(
                            "> Subiendo archivo local '{}' a la carpeta raíz...",
                            filename
                        )
                    } else {
                        format!(
                            "> Subiendo archivo local '{}' a la carpeta '{}'...",
                            filename, folder
                        )
                    },
                });

                match STANDARD.decode(&data_base64) {
                    Ok(bytes) => {
                        let file_path = target_dir.join(&filename);
                        if let Err(e) = tokio::fs::write(&file_path, bytes).await {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: format!("❌ Error guardando archivo: {}", e),
                            });
                            return;
                        }

                        let packwiz_bin = crate::system::deps::find_in_path("packwiz")
                            .unwrap_or_else(|| std::path::PathBuf::from("packwiz"));

                        let relative_file_path = if is_root {
                            filename.clone()
                        } else {
                            format!("{}/{}", folder, filename)
                        };

                        let out = tokio::process::Command::new(&packwiz_bin)
                            .arg("add")
                            .arg(&relative_file_path)
                            .current_dir(&dest_dir)
                            .output()
                            .await;

                        if let Ok(o) = out {
                            let log_out = String::from_utf8_lossy(&o.stdout);
                            if !log_out.is_empty() {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: log_out.to_string(),
                                });
                            }
                        }

                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("✅ Archivo '{}' trackeado exitosamente.", filename),
                        });

                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: None,
                        });
                    }
                    Err(_) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: "❌ Error decodificando archivo Base64.".into(),
                        });
                    }
                }
            });
        }

        ClientRequest::PublishPackwiz { id, pack_key, image } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            let target_clone = state.publish_target.clone();
            let domain_clone = state.domain.clone();
            tokio::spawn(async move {
                let pack_key = crate::docker::discovery::sanitize_container_name(&pack_key);
                let dest_dir = root_clone.join(&id);
                let pack_dir = dest_dir.join("packwiz");
                let database_dir = root_clone.join("lumineria_database");
                let packwiz_bin = crate::system::deps::find_in_path("packwiz")
                    .unwrap_or_else(|| std::path::PathBuf::from("packwiz"));

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("> Iniciando publicación de '{}'...", pack_key),
                });

                if let Err(e) =
                    ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx_clone)
                        .await
                {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ {}", e),
                    });
                    return;
                }

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: "> Regenerando base de datos global ('packwiz refresh')...".into(),
                });
                let refresh_out = tokio::process::Command::new(&packwiz_bin)
                    .arg("refresh")
                    .current_dir(&pack_dir)
                    .output()
                    .await;

                if let Ok(o) = refresh_out {
                    let log = String::from_utf8_lossy(&o.stdout);
                    if !log.is_empty() {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: log.to_string(),
                        });
                    }
                }

                let env_path = dest_dir.join("server.env");
                let env_data = fs::read_to_string(&env_path).await.unwrap_or_default();
                let mut title = "Servidor Lumineria".to_string();
                let mut mc_version = "1.21.1".to_string();
                let mut server_type = "neoforge".to_string();

                for line in env_data.lines() {
                    if line.starts_with("SERVER_NAME=") {
                        title = line
                            .replace("SERVER_NAME=", "")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("MC_VERSION=") {
                        mc_version = line
                            .replace("MC_VERSION=", "")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                    if line.starts_with("SERVER_TYPE=") {
                        server_type = line
                            .replace("SERVER_TYPE=", "")
                            .replace('"', "")
                            .trim()
                            .to_string();
                    }
                }

                let images_dir = root_clone.join("lumineria_database").join("images");
                let _ = tokio::fs::create_dir_all(&images_dir).await;

                let image_filename = if let Some(img) = image {
                    use base64::{engine::general_purpose::STANDARD, Engine as _};
                    match STANDARD.decode(&img.data_base64) {
                        Ok(bytes) => {
                            let ext = std::path::Path::new(&img.filename)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("png");
                            let saved_name = format!("{}.{}", pack_key, ext);
                            match tokio::fs::write(images_dir.join(&saved_name), bytes).await {
                                Ok(_) => {
                                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                                        id: id.clone(),
                                        line: format!(
                                            "🖼️ Imagen de portada guardada como '{}'.",
                                            saved_name
                                        ),
                                    });
                                    saved_name
                                }
                                Err(e) => {
                                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("⚠️ No pude guardar la imagen ({e}), uso la anterior o la de por defecto."),
                    });
                                    existing_image_filename(&images_dir, &pack_key)
                                        .await
                                        .unwrap_or_else(|| "smp.png".to_string())
                                }
                            }
                        }
                        Err(_) => {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "⚠️ Error decodificando la imagen enviada, uso la anterior o la de por defecto.".into(),
            });
                            existing_image_filename(&images_dir, &pack_key)
                                .await
                                .unwrap_or_else(|| "smp.png".to_string())
                        }
                    }
                } else {
                    existing_image_filename(&images_dir, &pack_key)
                        .await
                        .unwrap_or_else(|| "smp.png".to_string())
                };

                let entry = crate::installer::packwiz_db::ModpackEntry {
                    title,
                    mc_version: mc_version.clone(),
                    version_id: format!("{}-latest", server_type),
                    java_version: 21,
                    loader_name: server_type.to_uppercase(),
                    loader_url: "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.219/neoforge-21.1.219-installer.jar".to_string(),
                    packwiz_url: format!("{}/{}/pack.toml", domain_clone, pack_key),
                    image: format!("{}/images/{}", domain_clone, image_filename),
                };

                if let Err(e) =
                    crate::installer::packwiz_db::upsert_entry(&database_dir, &pack_key, entry)
                        .await
                {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ Fallo al generar modpacks.json: {}", e),
                    });
                    return;
                }
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: "✅ modpacks.json actualizado localmente.".into(),
                });

                match crate::publisher::publish_packwiz(
                    &target_clone,
                    &pack_dir,
                    &database_dir,
                    &pack_key,
                )
                .await
                {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: "🚀 Sincronización completada. El modpack está en línea.".into(),
                        });

                        let was_running = podman::is_running(&id).await;

                        if was_running {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: "⏸️ Deteniendo el servidor para actualizar mods...".into(),
                            });
                            if let Err(e) = podman::container_action("stop", &id).await {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: format!("❌ No pude detener el contenedor, aborto la sincronización: {e}"),
                                });
                                return;
                            }
                        }

                        let client = reqwest::Client::new();
                        let sync_result = installer::sync_server_mods(
                            &client,
                            "packwiz/pack.toml",
                            &dest_dir,
                            &id,
                            &tx_clone,
                        )
                        .await;

                        match sync_result {
                            Ok(()) => {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: "✅ Mods del servidor actualizados (archivos viejos eliminados).".into(),
                                });
                            }
                            Err(e) => {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: format!("❌ Error al sincronizar mods del servidor: {e}"),
                                });
                            }
                        }

                        if was_running {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: "▶️ Reiniciando el servidor...".into(),
                            });
                            if let Err(e) = podman::container_action("start", &id).await {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: format!("❌ No pude reiniciar el contenedor: {e}"),
                                });
                            }
                        }

                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: Some("Publicación y sincronización completadas".into()),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ Error en publicación: {}", e),
                        });
                    }
                }
            });
        }

        ClientRequest::ListPackwizMods { id } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let packwiz_dir = root_clone.join(&id).join("packwiz");
                let mut files_list = Vec::new();

                let categories = vec!["mods", "resourcepacks", "shaderpacks", "config"];

                for category in categories {
                    let target_dir = packwiz_dir.join(category);
                    if !target_dir.exists() {
                        continue;
                    }

                    if let Ok(mut entries) = tokio::fs::read_dir(target_dir).await {
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            let path = entry.path();
                            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                    let mut name = String::new();
                                    let mut filename = String::new();
                                    let mut side = "both".to_string();

                                    for line in content.lines() {
                                        let line = line.trim();
                                        if line.starts_with("name =") {
                                            name = line
                                                .split('=')
                                                .nth(1)
                                                .unwrap_or_default()
                                                .replace('"', "")
                                                .trim()
                                                .to_string();
                                        }
                                        if line.starts_with("filename =") {
                                            filename = line
                                                .split('=')
                                                .nth(1)
                                                .unwrap_or_default()
                                                .replace('"', "")
                                                .trim()
                                                .to_string();
                                        }
                                        if line.starts_with("side =") {
                                            side = line
                                                .split('=')
                                                .nth(1)
                                                .unwrap_or_default()
                                                .replace('"', "")
                                                .trim()
                                                .to_string();
                                        }
                                    }
                                    if !name.is_empty() {
                                        let display_filename = format!("{}/{}", category, filename);
                                        files_list.push(protocol::PackwizMod {
                                            name,
                                            filename: display_filename,
                                            side,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                files_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                let _ = tx_clone.send(ServerEvent::PackwizModsList {
                    id,
                    mods: files_list,
                });
            });
        }

        ClientRequest::UnpublishPackwiz { id, pack_key } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            let target_clone = state.publish_target.clone();
            tokio::spawn(async move {
                let pack_key = crate::docker::discovery::sanitize_container_name(&pack_key);
                let database_dir = root_clone.join("lumineria_database");
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("> Quitando publicación de '{}'...", pack_key),
                });

                let existed = match crate::installer::packwiz_db::remove_entry(
                    &database_dir,
                    &pack_key,
                )
                .await
                {
                    Ok(existed) => existed,
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ {}", e),
                        });
                        return;
                    }
                };
                if !existed {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: "ℹ️ Ese pack no estaba registrado en modpacks.json.".into(),
                    });
                }

                match crate::publisher::unpublish_packwiz(&target_clone, &database_dir, &pack_key)
                    .await
                {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: "🗑️ Publicación eliminada de Nginx y del VPS.".into(),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ Error al despublicar: {}", e),
                        });
                    }
                }
            });
        }

        ClientRequest::SendConsoleCommand { id, command } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let env_path = root_clone.join(&id).join("server.env");
                let env_data = match fs::read_to_string(&env_path).await {
                    Ok(d) => d,
                    Err(_) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: "No encontré la configuración del servidor.".into(),
                        });
                        return;
                    }
                };

                let rcon_port: u16 = read_env_value(&env_data, "RCON_PORT")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(25575);
                let rcon_password = match read_env_value(&env_data, "RCON_PASSWORD") {
                    Some(p) if !p.is_empty() => p,
                    _ => {
                        let _ = tx_clone.send(ServerEvent::Error {
                    message: "Este servidor no tiene RCON configurado (creálo de nuevo para tenerlo).".into(),
                });
                        return;
                    }
                };

                if !podman::is_running(&id).await {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: "El servidor está detenido, iniciálo antes de mandar comandos."
                            .into(),
                    });
                    return;
                }

                match crate::rcon::RconClient::connect("127.0.0.1", rcon_port, &rcon_password).await
                {
                    Ok(mut client) => match client.command(&command).await {
                        Ok(response) => {
                            let clean = if response.trim().is_empty() {
                                "(sin salida)".to_string()
                            } else {
                                response.trim().to_string()
                            };
                            let _ = tx_clone.send(ServerEvent::ConsoleResponse {
                                id,
                                response: clean,
                            });
                        }
                        Err(e) => {
                            let _ = tx_clone.send(ServerEvent::Error {
                                message: format!("Error ejecutando comando: {e}"),
                            });
                        }
                    },
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("No pude conectar al RCON: {e}"),
                        });
                    }
                }
            });
        }
    }
}

fn run_action(tx: &mpsc::UnboundedSender<ServerEvent>, id: &str, result: Result<()>) {
    let event = match result {
        Ok(()) => ServerEvent::Ack {
            ok: true,
            message: None,
        },
        Err(e) => ServerEvent::Error {
            message: format!("{id}: {e}"),
        },
    };
    let _ = tx.send(event);
}

fn read_env_value(env_data: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    env_data.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|v| v.trim().trim_matches('"').to_string())
    })
}

async fn existing_image_filename(images_dir: &std::path::Path, pack_key: &str) -> Option<String> {
    let mut entries = tokio::fs::read_dir(images_dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) == Some(pack_key) {
            return path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }
    }
    None
}
