use super::super::state::AppState;
use crate::docker::podman;
use protocol::ServerEvent;
use std::collections::HashMap;
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
        let mut first = true;
        loop {
            if line_tx.is_closed() {
                break;
            }
            let tail = if first { "100" } else { "0" };
            if let Err(e) = podman::stream_logs(container_id.clone(), tail, line_tx.clone()).await {
                tracing::warn!("stream_logs para '{container_id}' terminó con error: {e}");
            }
            first = false;
            if line_tx.is_closed() {
                break;
            }
            if !podman::is_running(&container_id).await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
    _state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    command: String,
) {
    let tx_clone = tx.clone();

    tokio::spawn(async move {
        if !podman::is_running(&id).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "El servidor está detenido, iniciálo antes de mandar comandos.".into(),
            });
            return;
        }

        // Enviamos el comando directamente por Podman, olvidándonos de RCON
        match podman::send_stdin_command(&id, &command).await {
            Ok(()) => {}
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("No pude enviar el comando: {e}"),
                });
            }
        }
    });
}
