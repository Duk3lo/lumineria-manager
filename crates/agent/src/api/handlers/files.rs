use super::super::state::AppState;
use super::super::utils::{base_dir_for_scope, safe_join};
use protocol::ServerEvent;
use tokio::sync::mpsc;

pub(crate) async fn list_files(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let base = base_dir_for_scope(&root_clone, &id, scope);
        let mut files = crate::build_file_tree(&base, "").await;

        if scope == protocol::FileScope::ServerRoot {
            let exclude = ["packwiz", "libraries", "cache", ".git"];
            files.retain(|n| !(n.is_dir && exclude.contains(&n.name.as_str())));
        }

        let _ = tx_clone.send(ServerEvent::PackwizFilesList { id, scope, files });
    });
}

pub(crate) async fn read_file(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    path: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let base = base_dir_for_scope(&root_clone, &id, scope);
        let file_path = match safe_join(&base, &path) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error { message: e });
                return;
            }
        };
        let content = tokio::fs::read_to_string(&file_path).await.ok();
        let _ = tx_clone.send(ServerEvent::FileContent {
            id,
            path,
            scope,
            content,
        });
    });
}

pub(crate) async fn write_file(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    path: String,
    content: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let base = base_dir_for_scope(&root_clone, &id, scope);
        let file_path = match safe_join(&base, &path) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error { message: e });
                return;
            }
        };
        if tokio::fs::write(&file_path, content).await.is_ok() {
            let _ = tx_clone.send(ServerEvent::Ack {
                ok: true,
                message: Some("Archivo guardado".into()),
            });
        } else {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "Error al guardar el archivo".into(),
            });
        }
    });
}

pub(crate) async fn delete_file(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    path: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let base = base_dir_for_scope(&root_clone, &id, scope);
        let file_path = match safe_join(&base, &path) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error { message: e });
                return;
            }
        };

        if file_path.is_dir() {
            let _ = tokio::fs::remove_dir_all(&file_path).await;
        } else {
            let _ = tokio::fs::remove_file(&file_path).await;
        }

        if scope == protocol::FileScope::Packwiz {
            let packwiz_bin = crate::system::deps::resolve_packwiz_bin();
            let _ = tokio::process::Command::new(&packwiz_bin)
                .arg("refresh")
                .current_dir(&base)
                .output()
                .await;
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some("Eliminado correctamente".into()),
        });
    });
}

pub(crate) async fn create_directory(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    path: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let base = base_dir_for_scope(&root_clone, &id, scope);
        let dir_path = match safe_join(&base, &path) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error { message: e });
                return;
            }
        };

        if let Err(e) = tokio::fs::create_dir_all(&dir_path).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("Error al crear carpeta: {}", e),
            });
        } else {
            let _ = tx_clone.send(ServerEvent::Ack {
                ok: true,
                message: Some(format!("Carpeta {} creada", path)),
            });
        }
    });
}