use super::super::state::AppState;
use protocol::{PluginSource, ServerEvent, VelocityPluginEntry};
use tokio::fs;
use tokio::sync::mpsc;

fn requirements_file_for(source: PluginSource) -> &'static str {
    match source {
        PluginSource::Modrinth => "modrinth.txt",
        PluginSource::Github => "github.txt",
        PluginSource::Direct => "direct_links.txt",
    }
}

fn requirements_dir(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join(id).join("requirements")
}

async fn read_lines(path: &std::path::Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path).await else {
        return Vec::new();
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub(crate) async fn list_plugins(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dir = requirements_dir(&root_clone, &id);
        let mut plugins = Vec::new();

        for source in [PluginSource::Modrinth, PluginSource::Github, PluginSource::Direct] {
            let path = dir.join(requirements_file_for(source));
            for value in read_lines(&path).await {
                plugins.push(VelocityPluginEntry { source, value });
            }
        }

        let _ = tx_clone.send(ServerEvent::VelocityPluginsList { id, plugins });
    });
}

pub(crate) async fn add_plugin(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    source: PluginSource,
    value: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let value = value.trim().to_string();
        if value.is_empty() {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "El valor no puede estar vacío.".into(),
            });
            return;
        }
        if value.contains('\n') || value.contains('\r') {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "El valor no puede contener saltos de línea.".into(),
            });
            return;
        }

        let dir = requirements_dir(&root_clone, &id);
        if let Err(e) = fs::create_dir_all(&dir).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No pude crear la carpeta requirements: {e}"),
            });
            return;
        }

        let path = dir.join(requirements_file_for(source));
        let existing = read_lines(&path).await;
        if existing.iter().any(|l| l.eq_ignore_ascii_case(&value)) {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("ℹ️ '{}' ya estaba en la lista, no se duplicó.", value),
            });
            let _ = tx_clone.send(ServerEvent::Ack { ok: true, message: None });
            return;
        }

        let mut new_lines = existing;
        new_lines.push(value.clone());
        let content = new_lines.join("\n") + "\n";

        if let Err(e) = fs::write(&path, content).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No pude guardar el archivo: {e}"),
            });
            return;
        }

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("✅ Añadido a {}: {}", requirements_file_for(source), value),
        });
        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some(
                "Plugin agregado. Se descarga automáticamente la próxima vez que el servidor arranque o se reinicie."
                    .into(),
            ),
        });
    });
}

pub(crate) async fn remove_plugin(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    source: PluginSource,
    value: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dir = requirements_dir(&root_clone, &id);
        let path = dir.join(requirements_file_for(source));
        let existing = read_lines(&path).await;
        let filtered: Vec<String> = existing
            .into_iter()
            .filter(|l| !l.eq_ignore_ascii_case(&value))
            .collect();

        let content = if filtered.is_empty() {
            String::new()
        } else {
            filtered.join("\n") + "\n"
        };

        if let Err(e) = fs::write(&path, content).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No pude guardar el archivo: {e}"),
            });
            return;
        }

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("🗑️ Quitado de {}: {}", requirements_file_for(source), value),
        });
        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some(
                "Plugin quitado de la lista. El .jar ya descargado sigue en /plugins hasta el próximo reinicio con limpieza."
                    .into(),
            ),
        });
    });
}

pub(crate) async fn set_mc_version_hint(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    mc_version: Option<String>,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);
        let value = mc_version.unwrap_or_default();
        if let Err(e) = crate::installer::config::update_env_key(
            &dest_dir,
            "VELOCITY_PLUGIN_MC_VERSION",
            &value,
        )
        .await
        {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No pude guardar la versión de referencia: {e}"),
            });
            return;
        }
        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some(if value.is_empty() {
                "Versión de referencia para Modrinth borrada.".into()
            } else {
                format!("Versión de referencia para Modrinth guardada: {value}")
            }),
        });
    });
}