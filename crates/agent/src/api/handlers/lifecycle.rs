use super::super::state::{with_busy_guard, AppState};
use super::super::utils::read_env_value;
use crate::docker::podman;
use crate::installer::{config, installer, plugin_downloader};
use anyhow::Result;
use protocol::{ServerConfigParams, ServerEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub(crate) async fn create_server(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    cfg: ServerConfigParams,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    let busy_clone = state.busy.clone();
    let tx_for_guard = tx.clone();
    tokio::spawn(async move {
        super::super::state::with_busy_guard(busy_clone, "install".to_string(), &tx_for_guard, async move {
        let dest_dir = root_clone.join(&id);
        if dest_dir.exists() {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!(
                    "❌ Ya existe un servidor con el id '{}'. Elige otro nombre.",
                    id
                ),
            });
            return;
        }

        let port_taken = crate::docker::discovery::is_port_registered(&root_clone, cfg.port)
            .unwrap_or(false);

        if port_taken {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!(
                    "❌ El puerto {} ya está en uso por otro servidor (esté corriendo o no). Elige otro.",
                    cfg.port
                ),
            });
            return;
        }

        if let Err(e) = fs::create_dir_all(&dest_dir).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No se pudo crear directorio: {e}"),
            });
            return;
        }
        let client = reqwest::Client::new();
        let _ = tx_clone.send(ServerEvent::InstallProgress {
            id: id.clone(),
            step: "Iniciando instalación".into(),
            percentage: 5,
        });

        // 👇 AQUI ESTABA EL ERROR: Ya NO pasamos rcon_creds
        if config::write_server_env(&dest_dir, &cfg).await.is_err()
            || config::write_server_properties(&dest_dir, &cfg).await.is_err()
        {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "Error al escribir configuraciones locales.".into(),
            });
            return;
        }
        
        let _ = tokio::fs::write(dest_dir.join("eula.txt"), "eula=true\n").await;
        let image = podman::java_image_for(&cfg.server_type, &cfg.mc_version);
        let result = match cfg.server_type.as_str() {
            "paper" | "velocity" | "folia" => installer::install_papermc(
                &client,
                &cfg.server_type,
                &cfg.mc_version,
                &dest_dir,
                &id,
                &tx_clone,
                &cfg.min_ram,
                &cfg.max_ram,
            )
            .await
            .map(|_| ()),
            "fabric" => {
                let loader = cfg.loader_version.clone().unwrap_or_default();
                installer::install_fabric(
                    &client,
                    &cfg.mc_version,
                    &loader,
                    &dest_dir,
                    &id,
                    &tx_clone,
                    &cfg.min_ram,
                    &cfg.max_ram,
                    image,
                )
                .await
            }
            "neoforge" => {
                let loader = cfg.loader_version.clone().unwrap_or_default();
                let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", loader);
                installer::install_mod_installer(
                    &url,
                    &format!("neoforge-{}-installer.jar", loader),
                    &dest_dir,
                    &id,
                    &tx_clone,
                    &cfg.min_ram,
                    &cfg.max_ram,
                    image,
                )
                .await
            }
            "forge" => {
                let loader = cfg.loader_version.clone().unwrap_or_default();
                let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar", loader);
                installer::install_mod_installer(
                    &url,
                    &format!("forge-{}-installer.jar", loader),
                    &dest_dir,
                    &id,
                    &tx_clone,
                    &cfg.min_ram,
                    &cfg.max_ram,
                    image,
                )
                .await
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            let _ = tokio::fs::remove_dir_all(&dest_dir).await;
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("Error de instalación: {e}. Se limpiaron los archivos parciales, podés reintentar con el mismo nombre."),
            });
            return;
        }

        if plugin_downloader::uses_direct_plugins(&cfg.server_type) {
            let _ = plugin_downloader::sync_plugins(&cfg.server_type, &dest_dir, &id, &tx_clone).await;
        }
        
        let _ = tx_clone.send(ServerEvent::InstallProgress {
            id: id.clone(),
            step: "Creando contenedor".into(),
            percentage: 90,
        });
        let image = podman::java_image_for(&cfg.server_type, &cfg.mc_version);
        match podman::create_container(&id, &dest_dir, image).await {
            Ok(()) => {
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: Some("Instalado exitosamente".into()),
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("Error: {e}"),
                });
            }
        }
        }).await;
    });
}

pub(crate) async fn auto_update_server(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();

    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);
        let env_path = dest_dir.join("server.env");

        let env_data = match fs::read_to_string(&env_path).await {
            Ok(data) => data,
            Err(_) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: "Instancia no válida para auto-actualización".into(),
                });
                return;
            }
        };

        let mut mc_version = "latest".to_string();
        let mut server_type = "paper".to_string();
        let mut min_ram = "1G".to_string();
        let mut max_ram = "4G".to_string();

        for line in env_data.lines() {
            if line.starts_with("MC_VERSION=") {
                mc_version = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("latest")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
            if line.starts_with("SERVER_TYPE=") {
                server_type = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("paper")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
            if line.starts_with("MIN_RAM=") {
                min_ram = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("1G")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
            if line.starts_with("MAX_RAM=") {
                max_ram = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("4G")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
        }

        if !matches!(server_type.as_str(), "paper" | "velocity" | "folia") {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "Auto-actualización solo disponible en motores Vanilla/PaperMC".into(),
            });
            return;
        }
        let was_running = podman::is_running(&id).await;
        if was_running {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "⏸️ Deteniendo el servidor para actualizar el motor...".into(),
            });
            if let Err(e) = podman::container_action("stop", &id).await {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("No pude detener el contenedor, aborto la actualización: {e}"),
                });
                return;
            }
        }

        let client = reqwest::Client::new();
        let _ = tx_clone.send(ServerEvent::InstallProgress {
            id: id.clone(),
            step: "Buscando actualizaciones estables...".into(),
            percentage: 20,
        });

        let result = installer::install_papermc(
            &client,
            &server_type,
            &mc_version,
            &dest_dir,
            &id,
            &tx_clone,
            &min_ram,
            &max_ram,
        )
        .await
        .map(|_| ());

        if let Err(e) = result {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("Error al actualizar: {e}"),
            });
            return;
        }
        let image = podman::java_image_for(&server_type, &mc_version);
        if let Err(e) = podman::create_container(&id, &dest_dir, image).await {
            let _ = tx_clone.send(ServerEvent::Error {
                message: format!("No pude recrear el contenedor: {e}"),
            });
            return;
        }

        if was_running {
            let _ = tx_clone.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "▶️ Reiniciando el servidor...".into(),
            });
            if let Err(e) = podman::container_action("start", &id).await {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("No pude reiniciar el contenedor: {e}"),
                });
                return;
            }
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some(if was_running {
                "Motor actualizado y servidor reiniciado.".into()
            } else {
                "Motor actualizado. Se usará al iniciar.".into()
            }),
        });
    });
}

pub(crate) async fn recreate_container(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        let dest_dir = root_clone.join(&id);
        let env_path = dest_dir.join("server.env");

        let env_data = match fs::read_to_string(&env_path).await {
            Ok(d) => d,
            Err(_) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: "No encontré la configuración.".into(),
                });
                return;
            }
        };

        let mut server_type = "paper".to_string();
        let mut mc_version = "latest".to_string();

        for line in env_data.lines() {
            if line.starts_with("SERVER_TYPE=") {
                server_type = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("paper")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
            if line.starts_with("MC_VERSION=") {
                mc_version = line
                    .split('=')
                    .nth(1)
                    .unwrap_or("latest")
                    .replace('"', "")
                    .trim()
                    .to_string();
            }
        }

        if !dest_dir.join("start.sh").exists() {
            let _ = tx_clone.send(ServerEvent::Error {
                message: "Falta start.sh — reinstala el motor antes de recrear.".into(),
            });
            return;
        }

        let image = podman::java_image_for(&server_type, &mc_version);
        match podman::create_container(&id, &dest_dir, image).await {
            Ok(()) => {
                let _ = tx_clone.send(ServerEvent::Ack {
                    ok: true,
                    message: Some("Contenedor recreado".into()),
                });
            }
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error {
                    message: format!("Error: {e}"),
                });
            }
        }
    });
}

pub(crate) async fn delete_server(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    log_tasks: &mut HashMap<String, JoinHandle<()>>,
    id: String,
) {
    if let Some(handle) = log_tasks.remove(&id) {
        handle.abort();
    }

    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        if let Err(e) = podman::delete_container(&id).await {
            tracing::warn!("Problema al borrar contenedor {id}: {e}");
        }

        let dest_dir = root_clone.join(&id);
        if dest_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&dest_dir).await {
                tracing::warn!(
                    "Fallo borrado normal de {id}: {e}. Intentando con podman unshare..."
                );
                let unshare_status = tokio::process::Command::new("podman")
                    .args(["unshare", "rm", "-rf", dest_dir.to_string_lossy().as_ref()])
                    .status()
                    .await;

                if unshare_status.is_err() || !unshare_status.unwrap().success() {
                    let _ = tx_clone.send(ServerEvent::Error {
                        message: format!(
                            "El contenedor se borró, pero no la carpeta (requiere sudo): {e}"
                        ),
                    });
                    return;
                }
            }
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some("Servidor y archivos eliminados exitosamente".into()),
        });
    });
}

pub(crate) async fn update_server_request(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    loader_version: Option<String>,
    update_mods: bool,
    update_engine: bool,
    force: bool,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    let busy_clone = state.busy.clone();
    let id_for_guard = id.clone();
    let tx_for_guard = tx.clone();

    tokio::spawn(async move {
        with_busy_guard(busy_clone, id_for_guard, &tx_for_guard, async move {
            update_server(
                id,
                loader_version,
                update_mods,
                update_engine,
                force,
                root_clone,
                tx_clone,
            )
            .await;
        })
        .await;
    });
}

async fn update_server(
    id: String,
    loader_version: Option<String>,
    update_mods: bool,
    update_engine: bool,
    force: bool,
    root: Arc<PathBuf>,
    tx: mpsc::UnboundedSender<ServerEvent>,
) {
    if !update_mods && !update_engine {
        let _ = tx.send(ServerEvent::Error {
            message: "No se seleccionó nada para actualizar.".into(),
        });
        return;
    }

    let dest_dir = root.join(&id);
    let env_path = dest_dir.join("server.env");
    let env_data = match fs::read_to_string(&env_path).await {
        Ok(d) => d,
        Err(_) => {
            let _ = tx.send(ServerEvent::Error {
                message: "No encontré la configuración del servidor.".into(),
            });
            return;
        }
    };

    let server_type = read_env_value(&env_data, "SERVER_TYPE").unwrap_or_else(|| "paper".into());
    let mut mc_version = read_env_value(&env_data, "MC_VERSION").unwrap_or_else(|| "latest".into());
    let min_ram = read_env_value(&env_data, "MIN_RAM").unwrap_or_else(|| "1G".into());
    let max_ram = read_env_value(&env_data, "MAX_RAM").unwrap_or_else(|| "4G".into());

    let pack_dir = dest_dir.join("packwiz");
    let packwiz_bin = crate::system::deps::resolve_packwiz_bin();

    let client = reqwest::Client::new();

    if server_type == "velocity" && update_engine {
        match installer::latest_velocity_version(&client).await {
            Ok(latest) => {
                if latest != mc_version {
                    let _ = tx.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!(
                            "🔎 Velocity: hay una versión más nueva disponible ({latest}), actualizando desde {mc_version}."
                        ),
                    });
                }
                mc_version = latest;
            }
            Err(e) => {
                let _ = tx.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!(
                        "⚠️ No pude chequear la última versión de Velocity ({e}), sigo con la configurada ({mc_version})."
                    ),
                });
            }
        }
    }

    let was_running = podman::is_running(&id).await;
    if was_running {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "⏸️ Deteniendo el servidor para actualizar...".into(),
        });
        if let Err(e) = podman::container_action("stop", &id).await {
            let _ = tx.send(ServerEvent::Error {
                message: format!("No pude detener el contenedor, aborto la actualización: {e}"),
            });
            return;
        }
    }

    let mut engine_detail: Option<String> = None;

    if update_mods {
        if let Err(e) =
            super::packwiz::ensure_packwiz_initialized(&dest_dir, &pack_dir, &packwiz_bin, &id, &tx)
                .await
        {
            let _ = tx.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("❌ {}", e),
            });
            if was_running {
                let _ = podman::container_action("start", &id).await;
            }
            return;
        }

        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "> Buscando actualizaciones de mods/plugins...".into(),
        });
        if let Ok(o) = tokio::process::Command::new(&packwiz_bin)
            .args(["update", "--all", "-y"])
            .current_dir(&pack_dir)
            .output()
            .await
        {
            let joined: String = String::from_utf8_lossy(&o.stdout)
                .lines()
                .chain(String::from_utf8_lossy(&o.stderr).lines())
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !joined.is_empty() {
                let _ = tx.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: joined,
                });
            }
        }
        let _ = tokio::process::Command::new(&packwiz_bin)
            .arg("refresh")
            .current_dir(&pack_dir)
            .output()
            .await;

        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "> Sincronizando mods/plugins actualizados al servidor...".into(),
        });
        if let Err(e) =
            installer::sync_server_mods(&client, "packwiz/pack.toml", &dest_dir, &id, &tx).await
        {
            let _ = tx.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!("❌ Error al sincronizar mods/plugins: {e}"),
            });
        }
    }

    if update_engine {
        let current_build = read_env_value(&env_data, "ENGINE_BUILD");

        let mut skip_reinstall = false;
        if !force {
            let latest_id = match server_type.as_str() {
                "paper" | "velocity" | "folia" => {
                    match installer::latest_papermc_build(&client, &server_type, &mc_version).await
                    {
                        Ok((_, build)) => Some(build),
                        Err(_) => None,
                    }
                }
                "fabric" | "neoforge" | "forge" => loader_version.clone(),
                _ => None,
            };

            if let (Some(latest), Some(current)) = (&latest_id, &current_build) {
                if latest == current {
                    skip_reinstall = true;
                }
            }
        }

        if skip_reinstall {
            let _ = tx.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: format!(
                    "✅ El motor ya está en la última compilación ({}). No hace falta reinstalar.",
                    current_build.unwrap_or_default()
                ),
            });
            engine_detail = Some(format!("{} (ya estaba al día)", mc_version));
        } else {
            let _ = tx.send(ServerEvent::PackwizLog {
                id: id.clone(),
                line: "🧹 Limpiando binarios del motor anterior...".into(),
            });
            if let Err(e) = clean_engine_files(&dest_dir).await {
                let _ = tx.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("⚠️ No pude limpiar del todo los archivos viejos: {e}"),
                });
            }

            let image = podman::java_image_for(&server_type, &mc_version);
            let mut new_build_id: Option<String> = None;

            let install_result: Result<()> = match server_type.as_str() {
                "paper" | "velocity" | "folia" => {
                    match installer::install_papermc(
                        &client,
                        &server_type,
                        &mc_version,
                        &dest_dir,
                        &id,
                        &tx,
                        &min_ram,
                        &max_ram,
                    )
                    .await
                    {
                        Ok((jar_name, build_number)) => {
                            engine_detail = Some(format!("{} ({})", mc_version, jar_name));
                            new_build_id = Some(build_number);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                "fabric" => {
                    let Some(loader) = loader_version.clone() else {
                        let _ = tx.send(ServerEvent::Error {
                            message: "Falta 'loader_version' para actualizar Fabric.".into(),
                        });
                        if was_running {
                            let _ = podman::container_action("start", &id).await;
                        }
                        return;
                    };
                    engine_detail = Some(format!("Fabric {} (MC {})", loader, mc_version));
                    new_build_id = Some(loader.clone());
                    installer::install_fabric(
                        &client,
                        &mc_version,
                        &loader,
                        &dest_dir,
                        &id,
                        &tx,
                        &min_ram,
                        &max_ram,
                        image,
                    )
                    .await
                }
                "neoforge" => {
                    let Some(loader) = loader_version.clone() else {
                        let _ = tx.send(ServerEvent::Error {
                            message: "Falta 'loader_version' para actualizar NeoForge.".into(),
                        });
                        if was_running {
                            let _ = podman::container_action("start", &id).await;
                        }
                        return;
                    };
                    engine_detail = Some(format!("NeoForge {} (MC {})", loader, mc_version));
                    new_build_id = Some(loader.clone());
                    let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", loader);
                    installer::install_mod_installer(
                        &url,
                        &format!("neoforge-{}-installer.jar", loader),
                        &dest_dir,
                        &id,
                        &tx,
                        &min_ram,
                        &max_ram,
                        image,
                    )
                    .await
                }
                "forge" => {
                    let Some(loader) = loader_version.clone() else {
                        let _ = tx.send(ServerEvent::Error {
                            message: "Falta 'loader_version' para actualizar Forge.".into(),
                        });
                        if was_running {
                            let _ = podman::container_action("start", &id).await;
                        }
                        return;
                    };
                    engine_detail = Some(format!("Forge {} (MC {})", loader, mc_version));
                    new_build_id = Some(loader.clone());
                    let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar", loader);
                    installer::install_mod_installer(
                        &url,
                        &format!("forge-{}-installer.jar", loader),
                        &dest_dir,
                        &id,
                        &tx,
                        &min_ram,
                        &max_ram,
                        image,
                    )
                    .await
                }
                other => {
                    let _ = tx.send(ServerEvent::Error {
                        message: format!("Motor '{other}' no soporta actualización automática."),
                    });
                    if was_running {
                        let _ = podman::container_action("start", &id).await;
                    }
                    return;
                }
            };

            if let Err(e) = install_result {
                let _ = tx.send(ServerEvent::Error {
                    message: format!("Error al reinstalar el motor: {e}"),
                });
                if was_running {
                    let _ = podman::container_action("start", &id).await;
                }
                return;
            }

            if let Some(detail) = &engine_detail {
                let _ = tx.send(ServerEvent::PackwizLog {
                    id: id.clone(),
                    line: format!("✔ Motor instalado: {detail}"),
                });
            }

            if let Some(build) = &new_build_id {
                if let Err(e) = config::update_env_key(&dest_dir, "ENGINE_BUILD", build).await {
                    let _ = tx.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("⚠️ No pude guardar el build instalado: {e}"),
                    });
                }
            }

            if server_type == "velocity" {
                if let Err(e) = config::update_env_key(&dest_dir, "MC_VERSION", &mc_version).await {
                    let _ = tx.send(ServerEvent::PackwizLog {
                        id: id.clone(),
                        line: format!("⚠️ No pude guardar la nueva versión de Velocity: {e}"),
                    });
                }
            }

            let image = podman::java_image_for(&server_type, &mc_version);
            if let Err(e) = podman::create_container(&id, &dest_dir, image).await {
                let _ = tx.send(ServerEvent::Error {
                    message: format!("No pude recrear el contenedor: {e}"),
                });
                if was_running {
                    let _ = podman::container_action("start", &id).await;
                }
                return;
            }
        }
    }

    if was_running {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.clone(),
            line: "▶️ Reiniciando el servidor...".into(),
        });
        if let Err(e) = podman::container_action("start", &id).await {
            let _ = tx.send(ServerEvent::Error {
                message: format!("No pude reiniciar el contenedor: {e}"),
            });
            return;
        }
    }

    let summary = match (update_mods, update_engine) {
        (true, true) => format!(
            "Servidor actualizado (mods/plugins + motor{})",
            engine_detail
                .as_deref()
                .map(|d| format!(": {d}"))
                .unwrap_or_default()
        ),
        (true, false) => "Mods/plugins actualizados y sincronizados".to_string(),
        (false, true) => format!(
            "Motor actualizado{}",
            engine_detail
                .as_deref()
                .map(|d| format!(": {d}"))
                .unwrap_or_default()
        ),
        (false, false) => unreachable!(),
    };
    let _ = tx.send(ServerEvent::Ack {
        ok: true,
        message: Some(summary),
    });
}

async fn clean_engine_files(dest_dir: &std::path::Path) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dest_dir).await?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jar") {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }
    let libraries = dest_dir.join("libraries");
    if libraries.is_dir() {
        let _ = tokio::fs::remove_dir_all(&libraries).await;
    }
    Ok(())
}