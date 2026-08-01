use super::super::state::AppState;
use crate::docker::podman;
use anyhow::Result;
use protocol::ServerEvent;
use tokio::sync::mpsc;

pub(crate) async fn list_servers(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    match crate::docker::discovery::discover(&state.root) {
        Ok(list) => {
            let _ = tx.send(ServerEvent::Servers { servers: list });
        }
        Err(e) => {
            let _ = tx.send(ServerEvent::Error {
                message: format!("no pude listar servidores: {e}"),
            });
        }
    }
}

pub(crate) async fn start_server(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    run_action(tx, &id, podman::container_action("start", &id).await);
}

pub(crate) async fn stop_server(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    run_action(tx, &id, podman::container_action("stop", &id).await);
}

pub(crate) async fn restart_server(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    run_action(tx, &id, podman::container_action("restart", &id).await);
}

pub(crate) async fn sync_mods(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    run_action(tx, &id, podman::sync_mods_now(&id).await);
}

pub(crate) async fn start_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    run_action(
        tx,
        "stack",
        podman::run_stack_script(&state.root, "start-podman.sh").await,
    );
}

pub(crate) async fn stop_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    run_action(
        tx,
        "stack",
        podman::run_stack_script(&state.root, "stop-podman.sh").await,
    );
}

pub(crate) async fn restart_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    run_action(
        tx,
        "stack",
        podman::run_stack_script(&state.root, "restart-podman.sh").await,
    );
}

pub(crate) fn run_action(tx: &mpsc::UnboundedSender<ServerEvent>, id: &str, result: Result<()>) {
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