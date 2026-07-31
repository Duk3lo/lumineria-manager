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
}

pub async fn serve(root: PathBuf, bind: String) -> Result<()> {
    let state = AppState {
        root: Arc::new(root),
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
            if log_tasks.contains_key(&id) {
                return;
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
                if config::write_server_env(&dest_dir, &cfg).await.is_err()
                    || config::write_server_properties(&dest_dir, &cfg)
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
                match podman::create_container(&id, &dest_dir, cfg.port, image).await {
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
                let mut port: u16 = 25565;

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
                    if line.starts_with("SERVER_PORT=") {
                        port = line
                            .split('=')
                            .nth(1)
                            .unwrap_or("25565")
                            .replace('"', "")
                            .trim()
                            .parse()
                            .unwrap_or(25565);
                    }
                }

                if !dest_dir.join("start.sh").exists() {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: "Falta start.sh — reinstala el motor antes de recrear.".into(),
                    });
                    return;
                }

                let image = podman::java_image_for(&server_type, &mc_version);
                match podman::create_container(&id, &dest_dir, port, image).await {
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
                // AQUÍ ES DONDE SE USA LA FUNCIÓN QUE TE MARCABA EL WARNING
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

                // 1. AUTO-INICIALIZACIÓN SI NO EXISTE
                if !pack_dir.join("pack.toml").exists() {
                    let _ = tokio::fs::create_dir_all(&pack_dir).await;
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: "⚠️ No se encontró un modpack. Auto-inicializando...".into(),
                    });

                    // Leer datos de server.env
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

                    // Mapear motor a formato packwiz (neoforge, forge, fabric, quilt)
                    let loader = match server_type.as_str() {
                        "paper" | "velocity" => "none",
                        other => other,
                    };

                    let init_status = tokio::process::Command::new(&packwiz_bin)
                        .args([
                            "init",
                            "--name",
                            &id,
                            "--author",
                            "Lumineria",
                            "--mc-version",
                            &mc_version,
                            "--modloader",
                            loader,
                            "-y",
                        ])
                        .current_dir(&pack_dir)
                        .status()
                        .await;

                    if init_status.is_err() || !init_status.unwrap().success() {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: "❌ Error al inicializar packwiz. ¿Está instalado go?".into(),
                        });
                        return;
                    }
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: "✅ Modpack inicializado correctamente con éxito.".into(),
                    });
                }

                // 2. AÑADIR EL MOD
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

                        // Refrescamos la lista de mods en el frontend de forma automática
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
            folder, // "mods", "config", "resourcepacks", "shaderpacks" o "." para la raíz
        } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let dest_dir = root_clone.join(&id).join("packwiz");

                // Detectar si el usuario quiere subir a la carpeta raíz
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

                        // Si es raíz, el archivo se añade directamente. Si no, va con su subcarpeta
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

                        // Refrescamos lista en el frontend
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

        ClientRequest::PublishPackwiz { id, pack_key } => {
            let tx_clone = tx.clone();
            let root_clone = state.root.clone();
            tokio::spawn(async move {
                let dest_dir = root_clone.join(&id);
                let pack_dir = dest_dir.join("packwiz");
                let database_dir = root_clone.join("lumineria_database");
                let packwiz_bin = crate::system::deps::find_in_path("packwiz")
                    .unwrap_or_else(|| std::path::PathBuf::from("packwiz"));

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("> Iniciando publicación de '{}'...", pack_key),
                });

                // 👇 1. REGENERAR ÍNDICES Y HASHES DE FORMA SEGURA 👇
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

                // 2. LEER DATOS LOCALES PARA ARMAR EL JSON AUTOMÁTICAMENTE
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

                let entry = crate::installer::packwiz_db::ModpackEntry {
                    title,
                    mc_version: mc_version.clone(),
                    version_id: format!("{}-latest", server_type),
                    java_version: 21,
                    loader_name: server_type.to_uppercase(),
                    loader_url: "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.219/neoforge-21.1.219-installer.jar".to_string(),
                    packwiz_url: format!("https://lumineria.duckdns.org/{}/pack.toml", pack_key),
                    image: "https://lumineria.duckdns.org/images/smp.png".to_string(),
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

                // 3. PUBLICAMOS POR SSH (Los archivos van con los hashes perfectos recalculados)
                let target = crate::publisher::PublishTarget {
                    ssh_host: "TheKaramelito@158.247.125.204".into(),
                    remote_base: "~/lumineria".into(),
                };

                match crate::publisher::publish_packwiz(
                    &target,
                    &pack_dir,
                    &database_dir,
                    &pack_key,
                )
                .await
                {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line:
                                "🚀 Sincronización completada. El modpack está en línea en Nginx."
                                    .into(),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: format!("❌ Error en SSH/Rsync: {}", e),
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

                // Escaneamos dinámicamente las carpetas clave de Packwiz
                let categories = vec!["mods", "resourcepacks", "shaderpacks", "config"];

                for category in categories {
                    let target_dir = packwiz_dir.join(category);
                    if !target_dir.exists() {
                        continue;
                    }

                    if let Ok(mut entries) = tokio::fs::read_dir(target_dir).await {
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            let path = entry.path();
                            // Packwiz trackea los archivos usando metadatos en archivos .toml
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
                                        // Guardamos la ruta relativa estructurada, ej: mods/sodium.jar o resourcepacks/fiel.zip
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
