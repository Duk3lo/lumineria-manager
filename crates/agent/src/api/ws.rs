use super::handlers::handle_request;
use super::state::AppState;
use crate::publisher::PublishTarget;
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
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

pub async fn serve(
    root: PathBuf,
    bind: String,
    publish_target: PublishTarget,
    domain: String,
    token: String,
) -> Result<()> {
    let state = AppState {
        root: Arc::new(root),
        publish_target: Arc::new(publish_target),
        domain: Arc::new(domain),
        token: Arc::new(token),
        busy: Arc::new(Mutex::new(HashSet::new())),
    };
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);
    tracing::info!("Agente escuchando en {bind} (recordá: solo detrás de un túnel SSH)");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let provided = params.get("token").map(String::as_str).unwrap_or("");
    if !token_matches(provided, &state.token) {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            "token inválido o ausente",
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
}

fn token_matches(provided: &str, expected: &str) -> bool {
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
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