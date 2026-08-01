mod console;
mod files;
mod lifecycle;
mod packwiz;
mod servers;

use super::state::AppState;
use protocol::{ClientRequest, ServerEvent};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) async fn handle_request(
    request: ClientRequest,
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    log_tasks: &mut HashMap<String, JoinHandle<()>>,
) {
    match request {
        ClientRequest::ListServers => servers::list_servers(state, tx).await,
        ClientRequest::StartServer { id } => servers::start_server(tx, id).await,
        ClientRequest::StopServer { id } => servers::stop_server(tx, id).await,
        ClientRequest::RestartServer { id } => servers::restart_server(tx, id).await,
        ClientRequest::SyncMods { id } => servers::sync_mods(tx, id).await,
        ClientRequest::StartStack => servers::start_stack(state, tx).await,
        ClientRequest::StopStack => servers::stop_stack(state, tx).await,
        ClientRequest::RestartStack => servers::restart_stack(state, tx).await,

        ClientRequest::SubscribeLogs { id } => console::subscribe_logs(tx, log_tasks, id).await,
        ClientRequest::UnsubscribeLogs { id } => console::unsubscribe_logs(log_tasks, id),
        ClientRequest::SendConsoleCommand { id, command } => {
            console::send_console_command(state, tx, id, command).await
        }

        ClientRequest::CreateServer { id, config } => {
            lifecycle::create_server(state, tx, id, config).await
        }
        ClientRequest::AutoUpdateServer { id } => lifecycle::auto_update_server(state, tx, id).await,
        ClientRequest::RecreateContainer { id } => {
            lifecycle::recreate_container(state, tx, id).await
        }
        ClientRequest::DeleteServer { id } => {
            lifecycle::delete_server(state, tx, log_tasks, id).await
        }
        ClientRequest::UpdateServer {
            id,
            loader_version,
            update_mods,
            update_engine,
            force,
        } => {
            lifecycle::update_server_request(
                state,
                tx,
                id,
                loader_version,
                update_mods,
                update_engine,
                force,
            )
            .await
        }

        ClientRequest::AddModPackwiz { id, query } => packwiz::add_mod(state, tx, id, query).await,
        ClientRequest::RemoveModPackwiz { id, query } => {
            packwiz::remove_mod(state, tx, id, query).await
        }
        ClientRequest::UploadModPackwiz {
            id,
            filename,
            data_base64,
            folder,
            scope,
        } => packwiz::upload_mod(state, tx, id, filename, data_base64, folder, scope).await,
        ClientRequest::PublishPackwiz {
            id,
            pack_key,
            image,
        } => packwiz::publish(state, tx, id, pack_key, image).await,
        ClientRequest::UnpublishPackwiz { id, pack_key } => {
            packwiz::unpublish(state, tx, id, pack_key).await
        }
        ClientRequest::ListPackwizMods { id } => packwiz::list_mods(state, tx, id).await,

        ClientRequest::ListPackwizFiles { id, scope } => {
            files::list_files(state, tx, id, scope).await
        }
        ClientRequest::ReadFile { id, path, scope } => {
            files::read_file(state, tx, id, path, scope).await
        }
        ClientRequest::WriteFile {
            id,
            path,
            content,
            scope,
        } => files::write_file(state, tx, id, path, content, scope).await,
        ClientRequest::DeleteFile { id, path, scope } => {
            files::delete_file(state, tx, id, path, scope).await
        }
        ClientRequest::CreateDirectory { id, path, scope } => {
            files::create_directory(state, tx, id, path, scope).await
        }
        ClientRequest::SyncPackToServer { id } => packwiz::sync_to_server(state, tx, id).await,
    }
}