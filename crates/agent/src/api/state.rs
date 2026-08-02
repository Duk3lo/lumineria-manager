use crate::publisher::PublishTarget;
use protocol::ServerEvent;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) root: Arc<PathBuf>,
    pub(crate) publish_target: Arc<RwLock<PublishTarget>>,
    pub(crate) domain: Arc<RwLock<String>>,
    pub(crate) token: Arc<String>,
    pub(crate) busy: Arc<Mutex<HashSet<String>>>,
}


pub(crate) async fn with_busy_guard<F>(
    busy: Arc<Mutex<HashSet<String>>>,
    id: String,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    fut: F,
) where
    F: std::future::Future<Output = ()>,
{
    {
        let mut set = busy.lock().await;
        if !set.insert(id.clone()) {
            let _ = tx.send(ServerEvent::Error {
                message: format!(
                    "Ya hay una operación en curso para '{}'. Esperá a que termine.",
                    id
                ),
            });
            return;
        }
    }
    fut.await;
    busy.lock().await.remove(&id);
}