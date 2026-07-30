use crate::{config_writer, installer};
use crate::{podman, servers};
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
        ClientRequest::ListServers => match servers::discover(&state.root) {
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
        ClientRequest::CreateServer { id, config } => {
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

                if config_writer::write_server_env(&dest_dir, &config)
                    .await
                    .is_err()
                    || config_writer::write_server_properties(&dest_dir, &config)
                        .await
                        .is_err()
                {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: "Error al escribir configuraciones locales.".into(),
                    });
                    return;
                }

                let result = match config.server_type.as_str() {
                    "paper" | "velocity" | "folia" => installer::install_papermc(
                        &client,
                        &config.server_type,
                        &config.mc_version,
                        &dest_dir,
                        &id,
                        &tx_clone,
                    )
                    .await
                    .map(|_| ()),

                    "fabric" => {
                        let loader = config.loader_version.clone().unwrap_or_default();
                        installer::install_fabric(
                            &client,
                            &config.mc_version,
                            &loader,
                            &dest_dir,
                            &id,
                            &tx_clone,
                        )
                        .await
                    }
                    "neoforge" => {
                        let loader = config.loader_version.clone().unwrap_or_default();
                        let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", loader);
                        installer::install_mod_installer(
                            &client,
                            &url,
                            &format!("neoforge-{}-installer.jar", loader),
                            &dest_dir,
                            &id,
                            &tx_clone,
                        )
                        .await
                    }
                    "forge" => {
                        let loader = config.loader_version.clone().unwrap_or_default();
                        let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar", loader);
                        installer::install_mod_installer(
                            &client,
                            &url,
                            &format!("forge-{}-installer.jar", loader),
                            &dest_dir,
                            &id,
                            &tx_clone,
                        )
                        .await
                    }
                    _ => Ok(()),
                };

                match result {
                    Ok(()) => {
                        let _ = tx_clone.send(ServerEvent::Ack {
                            ok: true,
                            message: Some("Instalado exitosamente".into()),
                        });
                    }
                    Err(e) => {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("Error de instalación: {e}"),
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

                if result.is_ok() {
                    let _ = tx_clone.send(ServerEvent::Ack {
                        ok: true,
                        message: Some(
                            "Motor actualizado al build recomendado de forma estable".into(),
                        ),
                    });
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
