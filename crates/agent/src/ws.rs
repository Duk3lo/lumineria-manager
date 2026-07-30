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

    // Canal interno: cualquier tarea (incluidas las de streaming de logs)
    // manda ServerEvent acá, y una única tarea se encarga de escribirlos
    // al websocket en orden.
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

    // Tareas de streaming de logs activas, por id de servidor, para poder
    // cancelarlas en un Unsubscribe o al desconectarse el cliente.
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
        ClientRequest::SyncMods { id } => {
            run_action(tx, &id, podman::sync_mods_now(&id).await)
        }

        ClientRequest::StartStack => {
            run_action(tx, "stack", podman::run_stack_script(&state.root, "start-podman.sh").await)
        }
        ClientRequest::StopStack => {
            run_action(tx, "stack", podman::run_stack_script(&state.root, "stop-podman.sh").await)
        }
        ClientRequest::RestartStack => {
            run_action(tx, "stack", podman::run_stack_script(&state.root, "restart-podman.sh").await)
        }

        ClientRequest::SubscribeLogs { id } => {
            if log_tasks.contains_key(&id) {
                return; // ya suscripto, no duplicamos la tarea
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
