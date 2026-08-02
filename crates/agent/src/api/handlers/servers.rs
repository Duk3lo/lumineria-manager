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


pub(crate) async fn stop_server(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    let tx = tx.clone();
    tokio::spawn(async move {
        run_action(&tx, &id, podman::container_action("stop", &id).await);
    });
}

pub(crate) async fn sync_mods(tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    let tx = tx.clone();
    tokio::spawn(async move {
        run_action(&tx, &id, podman::sync_mods_now(&id).await);
    });
}

pub(crate) async fn start_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    let tx = tx.clone();
    let root = state.root.clone();
    tokio::spawn(async move {
        run_action(&tx, "stack", podman::run_stack_script(&root, "start-podman.sh").await);
    });
}

pub(crate) async fn stop_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    let tx = tx.clone();
    let root = state.root.clone();
    tokio::spawn(async move {
        run_action(&tx, "stack", podman::run_stack_script(&root, "stop-podman.sh").await);
    });
}

pub(crate) async fn restart_stack(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>) {
    let tx = tx.clone();
    let root = state.root.clone();
    tokio::spawn(async move {
        run_action(&tx, "stack", podman::run_stack_script(&root, "restart-podman.sh").await);
    });
}

async fn read_server_type(root: &std::path::Path, id: &str) -> Option<String> {
    let data = tokio::fs::read_to_string(root.join(id).join("server.env")).await.ok()?;
    data.lines()
        .find_map(|l| l.strip_prefix("SERVER_TYPE="))
        .map(|v| v.trim().trim_matches('"').to_string())
}

pub(crate) async fn start_server(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    let tx = tx.clone();
    let root = state.root.clone();
    tokio::spawn(async move {
        if let Some(server_type) = read_server_type(&root, &id).await {
            if server_type == "velocity" {
                patch_velocity_config(&root, &id, &tx).await;
            }
            if crate::installer::plugin_downloader::uses_direct_plugins(&server_type) {
                let dest_dir = root.join(&id);
                if let Err(e) = crate::installer::plugin_downloader::sync_plugins(&server_type, &dest_dir, &id, &tx).await {
                    let _ = tx.send(ServerEvent::PackwizLog { id: id.clone(), line: format!("⚠️ No pude sincronizar plugins antes de arrancar: {e}") });
                }
            }
        }
        run_action(&tx, &id, podman::container_action("start", &id).await);
    });
}


pub(crate) async fn restart_server(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>, id: String) {
    let tx = tx.clone();
    let root = state.root.clone();
    tokio::spawn(async move {
        if let Some(server_type) = read_server_type(&root, &id).await {
            if server_type == "velocity" {
                patch_velocity_config(&root, &id, &tx).await;
            }
            if crate::installer::plugin_downloader::uses_direct_plugins(&server_type) {
                let dest_dir = root.join(&id);
                if let Err(e) = crate::installer::plugin_downloader::sync_plugins(&server_type, &dest_dir, &id, &tx).await {
                    let _ = tx.send(ServerEvent::PackwizLog { id: id.clone(), line: format!("⚠️ No pude sincronizar plugins antes de reiniciar: {e}") });
                }
            }
        }
        run_action(&tx, &id, podman::container_action("restart", &id).await);
    });
}

async fn patch_velocity_config(root: &std::path::Path, id: &str, tx: &mpsc::UnboundedSender<ServerEvent>) {
    let dest_dir = root.join(id);
    let env_data = tokio::fs::read_to_string(dest_dir.join("server.env")).await.unwrap_or_default();
    let port: u16 = env_data.lines()
        .find_map(|l| l.strip_prefix("SERVER_PORT="))
        .and_then(|v| v.trim().trim_matches('"').parse().ok())
        .unwrap_or(25577);
    let motd = env_data.lines()
        .find_map(|l| l.strip_prefix("SERVER_MOTD="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty());

    if let Err(e) = crate::installer::config::patch_velocity_toml(&dest_dir, port, motd.as_deref()).await {
        let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("⚠️ No pude ajustar velocity.toml: {e}") });
    }
}

pub(crate) async fn set_motd(state: &AppState, tx: &mpsc::UnboundedSender<ServerEvent>, id: String, motd: String) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);

        if let Err(e) = crate::installer::config::update_env_key(&dest_dir, "SERVER_MOTD", &motd).await {
            let _ = tx_clone.send(ServerEvent::Error { message: format!("No pude guardar el MOTD: {e}") });
            return;
        }

        let server_type = read_server_type(&root_clone, &id).await.unwrap_or_default();

        if server_type == "velocity" {
            let _ = tx_clone.send(ServerEvent::Ack {
                ok: true,
                message: Some("MOTD guardado. Se aplica en velocity.toml al iniciar/reiniciar el proxy.".into()),
            });
            return;
        }

        let props_path = dest_dir.join("server.properties");
        if let Ok(content) = tokio::fs::read_to_string(&props_path).await {
            let mut found = false;
            let new_lines: Vec<String> = content.lines().map(|line| {
                if line.starts_with("motd=") {
                    found = true;
                    format!("motd={motd}")
                } else {
                    line.to_string()
                }
            }).collect();
            let mut new_content = new_lines.join("\n") + "\n";
            if !found {
                new_content.push_str(&format!("motd={motd}\n"));
            }
            let _ = tokio::fs::write(&props_path, new_content).await;
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some("MOTD guardado en server.properties. Se aplica al reiniciar.".into()),
        });
    });
}

pub(crate) async fn set_port(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    port: u16,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        // ¿Otro servidor ya está usando este puerto?
        if let Ok(mut entries) = tokio::fs::read_dir(&*root_clone).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let other_id = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                if other_id == id { continue; }
                let other_env = path.join("server.env");
                if let Ok(data) = tokio::fs::read_to_string(&other_env).await {
                    let used_port = data.lines()
                        .find_map(|l| l.strip_prefix("SERVER_PORT="))
                        .map(|v| v.trim().trim_matches('"').to_string())
                        .and_then(|v| v.parse::<u16>().ok());
                    if used_port == Some(port) {
                        let _ = tx_clone.send(ServerEvent::Error {
                            message: format!("❌ El puerto {port} ya está en uso por el servidor '{other_id}'."),
                        });
                        return;
                    }
                }
            }
        }

        let dest_dir = root_clone.join(&id);

        if let Err(e) = crate::installer::config::update_env_key(&dest_dir, "SERVER_PORT", &port.to_string()).await {
            let _ = tx_clone.send(ServerEvent::Error { message: format!("No pude guardar el puerto: {e}") });
            return;
        }

        let server_type = read_server_type(&root_clone, &id).await.unwrap_or_default();

        if server_type == "velocity" {
            let _ = tx_clone.send(ServerEvent::Ack {
                ok: true,
                message: Some("Puerto guardado. Se aplica en velocity.toml al iniciar/reiniciar el proxy.".into()),
            });
            return;
        }

        let props_path = dest_dir.join("server.properties");
        if let Ok(content) = tokio::fs::read_to_string(&props_path).await {
            let mut found_port = false;
            let mut found_query = false;
            let new_lines: Vec<String> = content.lines().map(|line| {
                if line.starts_with("server-port=") {
                    found_port = true;
                    format!("server-port={port}")
                } else if line.starts_with("query.port=") {
                    found_query = true;
                    format!("query.port={port}")
                } else {
                    line.to_string()
                }
            }).collect();
            let mut new_content = new_lines.join("\n") + "\n";
            if !found_port { new_content.push_str(&format!("server-port={port}\n")); }
            if !found_query { new_content.push_str(&format!("query.port={port}\n")); }
            let _ = tokio::fs::write(&props_path, new_content).await;
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some("Puerto guardado en server.properties. Se aplica al reiniciar el servidor.".into()),
        });
    });
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