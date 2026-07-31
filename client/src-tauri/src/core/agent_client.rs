use futures_util::{SinkExt, StreamExt};
use protocol::{ClientRequest, ServerEvent};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub struct AgentConnection {
    tx: mpsc::UnboundedSender<ClientRequest>,
}

impl AgentConnection {
    pub fn send(&self, request: ClientRequest) -> Result<(), String> {
        self.tx.send(request).map_err(|e| e.to_string())
    }
}

pub async fn connect(app: AppHandle, url: String) -> anyhow::Result<AgentConnection> {
    let (ws_stream, _) = connect_async(&url).await?;
    let (mut write, mut read) = ws_stream.split();

    let (tx, mut rx) = mpsc::unbounded_channel::<ClientRequest>();

    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let Ok(json) = serde_json::to_string(&request) else { continue };
            if write.send(Message::Text(json.into())).await.is_err() { break; }
        }
    });

    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            let Message::Text(text) = msg else { continue };
            let Ok(event) = serde_json::from_str::<ServerEvent>(text.as_str()) else { continue };
            let _ = app.emit("server-event", event);
        }
    });

    Ok(AgentConnection { tx })
}