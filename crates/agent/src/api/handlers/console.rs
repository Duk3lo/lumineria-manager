use super::super::state::AppState;
use super::super::utils::read_env_value;
use crate::docker::podman;
use protocol::ServerEvent;
use std::collections::HashMap;
use tokio::fs;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) async fn subscribe_logs(
    tx: &mpsc::UnboundedSender<ServerEvent>,
    log_tasks: &mut HashMap<String, JoinHandle<()>>,
    id: String,
) {
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

pub(crate) fn unsubscribe_logs(log_tasks: &mut HashMap<String, JoinHandle<()>>, id: String) {
    if let Some(handle) = log_tasks.remove(&id) {
        handle.abort();
    }
}

pub(crate) async fn send_console_command(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    command: String,
) {
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
                    message:
                        "Este servidor no tiene RCON configurado (creálo de nuevo para tenerlo)."
                            .into(),
                });
                return;
            }
        };

        if !podman::is_running(&id).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "El servidor está detenido, iniciálo antes de mandar comandos.".into(),
            });
            return;
        }

        match crate::rcon::RconClient::connect("127.0.0.1", rcon_port, &rcon_password).await {
            Ok(mut client) => match client.command(&command).await {
                Ok(response) => {
                    let clean = if response.trim().is_empty() {
                        "(sin salida)".to_string()
                    } else {
                        response.trim().to_string()
                    };
                    let _ = tx_clone.send(ServerEvent::ConsoleResponse { id, response: clean });
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