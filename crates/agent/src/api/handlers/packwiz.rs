use super::super::state::{with_busy_guard, AppState};
use crate::docker::podman;
use crate::installer::installer;
use protocol::{PackwizImage, ServerEvent};
use tokio::fs;
use tokio::sync::mpsc;

fn display_loader_name(server_type: &str) -> String {
    match server_type.to_lowercase().as_str() {
        "neoforge" => "NeoForge".to_string(),
        "forge" => "Forge".to_string(),
        "fabric" => "Fabric".to_string(),
        "paper" => "Paper".to_string(),
        "velocity" => "Velocity".to_string(),
        "folia" => "Folia".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => other.to_string(),
            }
        }
    }
}

fn loader_installer_url(server_type: &str, build: &str) -> String {
    match server_type.to_lowercase().as_str() {
        "neoforge" => format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
            build
        ),
        "forge" => format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar",
            build
        ),
        _ => String::new(),
    }
}

fn version_id_for(server_type: &str, build: &Option<String>) -> String {
    match build {
        Some(b) if !b.is_empty() => format!("{}-{}", server_type.to_lowercase(), b),
        _ => format!("{}-desconocida", server_type.to_lowercase()),
    }
}

pub(crate) async fn ensure_packwiz_initialized(
    dest_dir: &std::path::Path,
    pack_dir: &std::path::Path,
    packwiz_bin: &std::path::Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> anyhow::Result<()> {
    if pack_dir.join("pack.toml").exists() {
        return Ok(());
    }
    tokio::fs::create_dir_all(pack_dir).await?;
    let _ = tx.send(ServerEvent::PackwizLog {
        id: id.to_string(),
        line: "⚠️ No se encontró un modpack. Auto-inicializando...".into(),
    });
    let env_path = dest_dir.join("server.env");
    let env_data = fs::read_to_string(&env_path).await.unwrap_or_default();
    let mut mc_version = "1.21.1".to_string();
    let mut server_type = "fabric".to_string();
    for line in env_data.lines() {
        if line.starts_with("MC_VERSION=") {
            mc_version = line
                .replace("MC_VERSION=", "")
                .replace('"', "")
                .trim()
                .to_string();
        }
        if line.starts_with("SERVER_TYPE=") {
            server_type = line
                .replace("SERVER_TYPE=", "")
                .replace('"', "")
                .trim()
                .to_string();
        }
    }

    let (loader, packwiz_mc_version): (&str, String) = match server_type.as_str() {
        "paper" => ("paper", mc_version.clone()),
        "velocity" => ("none", "1.21.1".to_string()),
        "folia" => ("paper", mc_version.clone()),
        other => (other, mc_version.clone()),
    };

    let output = tokio::process::Command::new(packwiz_bin)
        .args([
            "init",
            "--name",
            id,
            "--author",
            "Lumineria",
            "--mc-version",
            &packwiz_mc_version,
            "--modloader",
            loader,
            "-y",
        ])
        .current_dir(pack_dir)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        anyhow::bail!("No pude inicializar packwiz para '{}': {}", id, detail);
    }
    Ok(())
}

pub(crate) async fn add_mod(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    query: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);
        let pack_dir = dest_dir.join("packwiz");

        let packwiz_bin = crate::system::deps::resolve_packwiz_bin();

        if let Err(e) =
            ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx_clone).await
        {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("❌ {}", e),
            });
            return;
        }

        let source = if query.contains("curseforge.com") {
            "cf"
        } else {
            "modrinth"
        };
        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("> Añadiendo mod '{}'...", query),
        });

        let output = tokio::process::Command::new(&packwiz_bin)
            .arg(source)
            .arg("add")
            .arg(&query)
            .arg("-y")
            .current_dir(&pack_dir)
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                if !stdout.is_empty() {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: stdout.trim().to_string(),
                    });
                }
                if !stderr.is_empty() {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: stderr.trim().to_string(),
                    });
                }
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: None,
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("❌ Error: {}", e),
                });
            }
        }
    });
}

pub(crate) async fn remove_mod(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    query: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id).join("packwiz");
        let packwiz_bin = crate::system::deps::resolve_packwiz_bin();

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("> Eliminando mod '{}'...", query),
        });
        let output = tokio::process::Command::new(&packwiz_bin)
            .arg("remove")
            .arg(&query)
            .current_dir(&dest_dir)
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                if !stdout.is_empty() {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: stdout.trim().to_string(),
                    });
                }
                if !stderr.is_empty() {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: stderr.trim().to_string(),
                    });
                }
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: None,
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("❌ Error: {}", e),
                });
            }
        }
    });
}

pub(crate) async fn upload_mod(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    filename: String,
    data_base64: String,
    folder: String,
    scope: protocol::FileScope,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let filename_lower = filename.to_lowercase();
        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
            || matches!(filename_lower.as_str(), "pack.toml" | "index.toml")
        {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("❌ Nombre de archivo inválido o reservado: '{}'", filename),
            });
            return;
        }

        let base = crate::api::utils::base_dir_for_scope(&root_clone, &id, scope);
        let is_root = folder.is_empty() || folder == "." || folder == "root";
        let target_dir = if is_root {
            base.clone()
        } else {
            match crate::api::utils::safe_join(&base, &folder) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ {}", e),
                    });
                    return;
                }
            }
        };
        let _ = tokio::fs::create_dir_all(&target_dir).await;

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("> Subiendo archivo '{}'...", filename),
        });

        match STANDARD.decode(&data_base64) {
            Ok(bytes) => {
                let file_path = target_dir.join(&filename);
                if let Err(e) = tokio::fs::write(&file_path, bytes).await {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ Error guardando archivo: {}", e),
                    });
                    return;
                }

                if scope == protocol::FileScope::Packwiz {
                    let packwiz_bin = crate::system::deps::resolve_packwiz_bin();
                    let out = tokio::process::Command::new(&packwiz_bin)
                        .arg("refresh")
                        .current_dir(&base)
                        .output()
                        .await;
                    if let Ok(o) = out {
                        let log_out = String::from_utf8_lossy(&o.stdout);
                        if !log_out.is_empty() {
                            let _ = tx_clone.send(ServerEvent::PackwizLog {
                                id: id.clone(),
                                line: log_out.to_string(),
                            });
                        }

                        // === NUEVA AYUDITA DE AUTODETECCIÓN PARA SUBIDAS MANUALES ===
                        let folder_lower = folder.to_lowercase();
                        if folder_lower.contains("resourcepacks") || folder_lower.contains("shaderpacks") {
                            let relative_file_path = if is_root {
                                filename.clone()
                            } else {
                                format!("{}/{}", folder, filename)
                            };
                            
                            let index_path = base.join("index.toml");
                            if let Ok(index_content) = tokio::fs::read_to_string(&index_path).await {
                                let mut new_content = String::new();
                                let mut in_target_file = false;
                                let mut added_meta = false;

                                for line in index_content.lines() {
                                    if line.trim().starts_with("file =") {
                                        let f = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                                        if f == relative_file_path {
                                            in_target_file = true;
                                            added_meta = false;
                                        } else {
                                            in_target_file = false;
                                        }
                                    }

                                    if in_target_file && line.trim().starts_with("[[files]]") {
                                        if !added_meta {
                                            new_content.push_str("meta.side = \"client\"\n");
                                        }
                                        in_target_file = false;
                                    }

                                    new_content.push_str(line);
                                    new_content.push('\n');
                                }
                                if in_target_file && !added_meta {
                                    new_content.push_str("meta.side = \"client\"\n");
                                }

                                let _ = tokio::fs::write(&index_path, new_content).await;

                                // Refrescar de nuevo para validar el index.toml limpio
                                let _ = tokio::process::Command::new(&packwiz_bin)
                                    .arg("refresh")
                                    .current_dir(&base)
                                    .output()
                                    .await;
                            }
                        }
                    }
                }

                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("✅ Archivo '{}' subido exitosamente.", filename),
                });
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: None,
                });
            }
            Err(_) => {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: "❌ Error decodificando archivo Base64.".into(),
                });
            }
        }
    });
}

pub(crate) async fn publish(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    pack_key: String,
    image: Option<PackwizImage>,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    let target_arc = state.publish_target.clone();
    let domain_arc = state.domain.clone();
    let busy_clone = state.busy.clone();
    let domain_value = domain_arc.read().await.clone();
    let id_for_guard = id.clone();
    let tx_for_guard = tx.clone();

    tokio::spawn(async move {
        with_busy_guard(busy_clone, id_for_guard, &tx_for_guard, async move {
            let pack_key = crate::docker::discovery::sanitize_container_name(&pack_key);
            let dest_dir = root_clone.join(&id);
            let pack_dir = dest_dir.join("packwiz");
            let database_dir = root_clone.join("lumineria_database");
            let packwiz_bin = crate::system::deps::resolve_packwiz_bin();

            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("> Iniciando publicación de '{}'...", pack_key),
            });

            if let Err(e) =
                ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx_clone)
                    .await
            {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("❌ {}", e),
                });
                return;
            }

            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "> Regenerando base de datos global ('packwiz refresh')...".into(),
            });
            let refresh_out = tokio::process::Command::new(&packwiz_bin)
                .arg("refresh")
                .current_dir(&pack_dir)
                .output()
                .await;

            if let Ok(o) = refresh_out {
                let log = String::from_utf8_lossy(&o.stdout);
                if !log.is_empty() {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: log.to_string(),
                    });
                }
            }

            let env_path = dest_dir.join("server.env");
            let env_data = fs::read_to_string(&env_path).await.unwrap_or_default();
            let mut title = "Servidor Lumineria".to_string();
let mut mc_version = "1.21.1".to_string();
let mut server_type = "neoforge".to_string();
let mut engine_build: Option<String> = None; 

for line in env_data.lines() {
    if line.starts_with("SERVER_NAME=") {
        title = line
            .replace("SERVER_NAME=", "")
            .replace('"', "")
            .trim()
            .to_string();
    }
    if line.starts_with("MC_VERSION=") {
        mc_version = line
            .replace("MC_VERSION=", "")
            .replace('"', "")
            .trim()
            .to_string();
    }
    if line.starts_with("SERVER_TYPE=") {
        server_type = line
            .replace("SERVER_TYPE=", "")
            .replace('"', "")
            .trim()
            .to_string();
    }
    if line.starts_with("ENGINE_BUILD=") {
        engine_build = Some(
            line.replace("ENGINE_BUILD=", "")
                .replace('"', "")
                .trim()
                .to_string(),
        );
    }
}

            let images_dir = root_clone.join("lumineria_database").join("images");
            let _ = tokio::fs::create_dir_all(&images_dir).await;

            let image_filename = if let Some(img) = image {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                match STANDARD.decode(&img.data_base64) {
                    Ok(bytes) => {
                        let ext = std::path::Path::new(&img.filename)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("png");
                        let saved_name = format!("{}.{}", pack_key, ext);
                        match tokio::fs::write(images_dir.join(&saved_name), bytes).await {
                            Ok(_) => {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: format!(
                                        "🖼️ Imagen de portada guardada como '{}'.",
                                        saved_name
                                    ),
                                });
                                saved_name
                            }
                            Err(e) => {
                                let _ = tx_clone.send(ServerEvent::PackwizLog {
                                    id: id.clone(),
                                    line: format!(
                                        "⚠️ No pude guardar la imagen ({e}), uso la anterior o la de por defecto."
                                    ),
                                });
                                existing_image_filename(&images_dir, &pack_key)
                                    .await
                                    .unwrap_or_else(|| "smp.png".to_string())
                            }
                        }
                    }
                    Err(_) => {
                        let _ = tx_clone.send(ServerEvent::PackwizLog {
                            id: id.clone(),
                            line: "⚠️ Error decodificando la imagen enviada, uso la anterior o la de por defecto.".into(),
                        });
                        existing_image_filename(&images_dir, &pack_key)
                            .await
                            .unwrap_or_else(|| "smp.png".to_string())
                    }
                }
            } else {
                existing_image_filename(&images_dir, &pack_key)
                    .await
                    .unwrap_or_else(|| "smp.png".to_string())
            };

            let build_str = engine_build.clone().unwrap_or_default();
let entry = crate::installer::packwiz_db::ModpackEntry {
    title,
    mc_version: mc_version.clone(),
    version_id: version_id_for(&server_type, &engine_build),
    java_version: 21,
    loader_name: display_loader_name(&server_type),
    loader_url: loader_installer_url(&server_type, &build_str),
    packwiz_url: format!("{}/{}/pack.toml", domain_value, pack_key),
    image: format!("{}/images/{}", domain_value, image_filename),
};

            if let Err(e) =
                crate::installer::packwiz_db::upsert_entry(&database_dir, &pack_key, entry).await
            {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("❌ Fallo al generar modpacks.json: {}", e),
                });
                return;
            }
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "✅ modpacks.json actualizado localmente.".into(),
            });

            let target_value = target_arc.read().await.clone();
match crate::publisher::publish_packwiz(&target_value, &pack_dir, &database_dir, &pack_key).await {
    Ok(()) => {
        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "🚀 Sincronización completada. El modpack está en línea.".into(),
        });

                    match sync_mods_to_running_server(&dest_dir, &id, &tx_clone).await {
    Ok(()) => {
        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "✅ Mods del servidor actualizados (archivos viejos eliminados).".into(),
        });
    }
    Err(e) => {
        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("❌ Error al sincronizar mods del servidor: {e}"),
        });
    }
}

let _ = tx_clone.send(ServerEvent::Ack {
    ok: true,
    message: Some("Publicación y sincronización completadas".into()),
});
                }
                Err(e) => {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ Error en publicación: {}", e),
                    });
                }
            }
        })
        .await;
    });
}

pub(crate) async fn unpublish(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    pack_key: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    let target_arc = state.publish_target.clone();
    tokio::spawn(async move {
        let pack_key = crate::docker::discovery::sanitize_container_name(&pack_key);
        let database_dir = root_clone.join("lumineria_database");
        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("> Quitando publicación de '{}'...", pack_key),
        });

        let existed =
            match crate::installer::packwiz_db::remove_entry(&database_dir, &pack_key).await {
                Ok(existed) => existed,
                Err(e) => {
                    let _ = tx_clone.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("❌ {}", e),
                    });
                    return;
                }
            };

        if !existed {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "ℹ️ Ese pack no estaba registrado en modpacks.json.".into(),
            });
        }

        let target_value = target_arc.read().await.clone();

        match crate::publisher::unpublish_packwiz(&target_value, &database_dir, &pack_key).await {
            Ok(()) => {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: "🗑️ Publicación eliminada de Nginx y del VPS.".into(),
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("❌ Error al despublicar: {}", e),
                });
            }
        }
    });
}



pub(crate) async fn list_mods(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let packwiz_dir = root_clone.join(&id).join("packwiz");
        let index_path = packwiz_dir.join("index.toml");
        
        let mut files_list = Vec::new();
        let mut index_sides: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        
        // Leemos index.toml para saber qué lado tienen los .jar custom
        if let Ok(index_content) = tokio::fs::read_to_string(&index_path).await {
            let mut current_file = String::new();
            for line in index_content.lines() {
                let line = line.trim();
                if line.starts_with("file =") {
                    current_file = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                }
                if line.starts_with("meta.side =") {
                    let side = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                    if !current_file.is_empty() {
                        index_sides.insert(current_file.clone(), side);
                    }
                }
            }
        }

        let categories = vec!["mods", "resourcepacks", "shaderpacks", "plugins", "config"];

        for category in categories {
            let target_dir = packwiz_dir.join(category);
            if !target_dir.exists() { continue; }

            if let Ok(mut entries) = tokio::fs::read_dir(&target_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let display_filename = format!("{}/{}", category, name);

                    if ext == "toml" {
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            let mut mod_name = String::new();
                            let mut mod_filename = String::new();
                            let mut side = "both".to_string();

                            for line in content.lines() {
                                let line = line.trim();
                                if line.starts_with("name =") {
                                    mod_name = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                                }
                                if line.starts_with("filename =") {
                                    mod_filename = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                                }
                                if line.starts_with("side =") {
                                    side = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                                }
                            }
                            if !mod_name.is_empty() {
                                let toml_path_str = format!("{}/{}", category, name);
                                let final_filename = format!("{}/{}", category, mod_filename);
                                files_list.push(protocol::PackwizMod {
                                    name: mod_name,
                                    filename: final_filename,
                                    toml_path: toml_path_str,
                                    side,
                                });
                            }
                        }
                    } else if ext == "jar" || ext == "zip" {
                        // Es un archivo RAW (Custom Mod)
                        let side = index_sides.get(&display_filename).cloned().unwrap_or_else(|| "both".to_string());
                        files_list.push(protocol::PackwizMod {
                            name: name.clone(),
                            filename: display_filename.clone(),
                            toml_path: display_filename, // Usamos la ruta del archivo mismo
                            side,
                        });
                    }
                }
            }
        }


        let mut referenced_jars = std::collections::HashSet::new();
        for m in &files_list {
            if m.toml_path.ends_with(".toml") {
                referenced_jars.insert(m.filename.clone());
            }
        }
        files_list.retain(|m| m.toml_path.ends_with(".toml") || !referenced_jars.contains(&m.filename));

        files_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let _ = tx_clone.send(ServerEvent::PackwizModsList {
            id,
            mods: files_list,
        });
    });
}

async fn existing_image_filename(images_dir: &std::path::Path, pack_key: &str) -> Option<String> {
    let mut entries = tokio::fs::read_dir(images_dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) == Some(pack_key) {
            return path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }
    }
    None
}

async fn sync_mods_to_running_server(
    dest_dir: &std::path::Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> anyhow::Result<()> {
    let was_running = podman::is_running(id).await;

    if was_running {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.to_string(),
            line: "⏸️ Deteniendo el servidor para actualizar mods...".into(),
        });
        podman::container_action("stop", id).await?;
    }

    let client = reqwest::Client::new();
    let sync_result =
        installer::sync_server_mods(&client, "packwiz/pack.toml", dest_dir, id, tx).await;

    if was_running {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.to_string(),
            line: "▶️ Reiniciando el servidor...".into(),
        });
        podman::container_action("start", id).await?;
    }

    sync_result
}

pub(crate) async fn sync_to_server(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);
        let pack_dir = dest_dir.join("packwiz");
        let packwiz_bin = crate::system::deps::resolve_packwiz_bin();

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line:
                "> Sincronizando mods/plugins solo con este servidor (sin publicar a clientes)..."
                    .into(),
        });

        if let Err(e) =
            ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx_clone).await
        {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("❌ {}", e),
            });
            return;
        }

        if let Ok(o) = tokio::process::Command::new(&packwiz_bin)
            .arg("refresh")
            .current_dir(&pack_dir)
            .output()
            .await
        {
            let log = String::from_utf8_lossy(&o.stdout);
            if !log.is_empty() {
                let _ = tx_clone.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: log.to_string(),
                });
            }
        }

        match sync_mods_to_running_server(&dest_dir, &id, &tx_clone).await {
            Ok(()) => {
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: Some("Mods/plugins sincronizados solo en este servidor (no se publicó nada para clientes).".into()),
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("Error al sincronizar mods/plugins con el servidor: {e}"),
                });
            }
        }
    });
}

pub(crate) async fn change_mod_side(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    toml_path: String,
    side: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let packwiz_dir = root_clone.join(&id).join("packwiz");
        
        if toml_path.ends_with(".toml") {
            // Mod Oficial: Editamos su archivo .toml
            let full_path = match crate::api::utils::safe_join(&packwiz_dir, &toml_path) {
                Ok(p) => p,
                Err(e) => { let _ = tx_clone.send(ServerEvent::Error { message: e }); return; }
            };

            if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                let mut found = false;
                let mut new_lines: Vec<String> = content.lines().map(|line| {
                    if line.trim_start().starts_with("side =") {
                        found = true;
                        format!("side = \"{}\"", side)
                    } else {
                        line.to_string()
                    }
                }).collect();

                if !found { new_lines.push(format!("side = \"{}\"", side)); }

                let _ = tokio::fs::write(&full_path, new_lines.join("\n") + "\n").await;
            }
        } else {
            // Mod Custom (.jar): Editamos el index.toml
            let index_path = packwiz_dir.join("index.toml");
            if let Ok(content) = tokio::fs::read_to_string(&index_path).await {
                let mut new_content = String::new();
                let mut in_target_file = false;
                let mut added_meta = false;
                
                for line in content.lines() {
                    if line.trim().starts_with("file =") {
                        let f = line.split('=').nth(1).unwrap_or_default().replace('"', "").trim().to_string();
                        if f == toml_path {
                            in_target_file = true;
                            added_meta = false;
                        } else {
                            in_target_file = false;
                        }
                    }
                    
                    if in_target_file && line.trim().starts_with("meta.side =") {
                        if side == "both" { continue; } // Si es both, borramos la regla
                        else {
                            new_content.push_str(&format!("meta.side = \"{}\"\n", side));
                            added_meta = true;
                            continue;
                        }
                    }
                    
                    if in_target_file && line.trim().starts_with("[[files]]") {
                        if !added_meta && side != "both" {
                            new_content.push_str(&format!("meta.side = \"{}\"\n", side));
                        }
                        in_target_file = false;
                    }

                    new_content.push_str(line);
                    new_content.push('\n');
                }
                if in_target_file && !added_meta && side != "both" {
                    new_content.push_str(&format!("meta.side = \"{}\"\n", side));
                }

                let _ = tokio::fs::write(&index_path, new_content).await;
            }
        }

        // Refrescar packwiz para que aplique los cambios
        let packwiz_bin = crate::system::deps::resolve_packwiz_bin();
        let _ = tokio::process::Command::new(&packwiz_bin)
            .arg("refresh")
            .current_dir(&packwiz_dir)
            .output()
            .await;

        let _ = tx_clone.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: format!("✅ Lado cambiado a '{}' para {}.", side, toml_path),
        });
        
        // Esto le dice al frontend que recargue la lista automáticamente
        let _ = tx_clone.send(ServerEvent::Ack { ok: true, message: None });
    });
}