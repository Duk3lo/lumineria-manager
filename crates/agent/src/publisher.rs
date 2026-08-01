use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub enum PublishTarget {
    /// El agente corre en OTRA máquina: hay que salir por SSH (sigue usando rsync, es transferencia de red real).
    Ssh {
        ssh_host: String,
        remote_base: String,
        web_root: String,
    },
    /// El agente YA corre en el mismo servidor que sirve la web: todo es copia de archivos local, sin sudo ni scripts externos.
    LocalFs {
        base_path: PathBuf,
        web_root: PathBuf,
    },
}

pub async fn publish_packwiz(
    target: &PublishTarget,
    local_pack_dir: &Path,
    local_database_dir: &Path,
    pack_key: &str,
) -> Result<()> {
    match target {
        PublishTarget::Ssh {
            ssh_host,
            remote_base,
            web_root,
        } => {
            tracing::info!("Publicando '{pack_key}' por SSH hacia {ssh_host}");
            let remote_pack = format!("{remote_base}/lumineria_packwiz/{pack_key}/");
            rsync(local_pack_dir, &remote_pack, Some(ssh_host)).await?;

            let remote_db = format!("{remote_base}/lumineria_database/");
            rsync(local_database_dir, &remote_db, Some(ssh_host)).await?;

            // En vez de depender de un publish.sh remoto, armamos el comando acá mismo.
            let cmd = format!(
                "mkdir -p '{web_root}' '{web_root}/images' '{web_root}/{pack_key}' && \
                 cp '{remote_base}/lumineria_database/modpacks.json' '{web_root}/' && \
                 (cp -r '{remote_base}/lumineria_database/images/.' '{web_root}/images/' 2>/dev/null || true) && \
                 rm -rf '{web_root}/{pack_key}' && \
                 cp -r '{remote_base}/lumineria_packwiz/{pack_key}' '{web_root}/{pack_key}' && \
                 chmod -R 755 '{web_root}'"
            );
            run_remote(&cmd, ssh_host).await?;
        }
        PublishTarget::LocalFs {
            base_path,
            web_root,
        } => {
            tracing::info!("Publicando '{pack_key}' localmente (sin sudo, sin script externo)");

            // 1. Espejar el pack y la base de datos dentro de base_path (igual que hacía rsync --delete)
            let pack_dest = base_path.join("lumineria_packwiz").join(pack_key);
            mirror_dir(local_pack_dir, &pack_dest).await?;

            let db_dest = base_path.join("lumineria_database");
            mirror_dir(local_database_dir, &db_dest).await?;

            // 2. Publicar al web root
            fs::create_dir_all(web_root)
                .await
                .with_context(|| format!("no pude crear {}", web_root.display()))?;

            let db_json_src = db_dest.join("modpacks.json");
            if db_json_src.exists() {
                fs::copy(&db_json_src, web_root.join("modpacks.json"))
                    .await
                    .context("no pude copiar modpacks.json al web root")?;
            }

            let images_src = db_dest.join("images");
            if images_src.is_dir() {
                copy_dir_recursive(&images_src, &web_root.join("images")).await?;
            }

            let pack_web_dest = web_root.join(pack_key);
            if pack_web_dest.exists() {
                fs::remove_dir_all(&pack_web_dest).await.ok();
            }
            copy_dir_recursive(&pack_dest, &pack_web_dest).await?;

            set_permissions_recursive(web_root, 0o755).await.ok();
        }
    }
    Ok(())
}

pub async fn unpublish_packwiz(
    target: &PublishTarget,
    local_database_dir: &Path,
    pack_key: &str,
) -> Result<()> {
    match target {
        PublishTarget::Ssh {
            ssh_host,
            remote_base,
            web_root,
        } => {
            tracing::info!("Despublicando '{pack_key}' de {ssh_host}");
            let remote_db = format!("{remote_base}/lumineria_database/");
            rsync(local_database_dir, &remote_db, Some(ssh_host)).await?;

            let cmd = format!(
                "cp '{remote_base}/lumineria_database/modpacks.json' '{web_root}/' && \
                 rm -rf '{remote_base}/lumineria_packwiz/{pack_key}' '{web_root}/{pack_key}'"
            );
            run_remote(&cmd, ssh_host).await?;
        }
        PublishTarget::LocalFs {
            base_path,
            web_root,
        } => {
            tracing::info!("Despublicando '{pack_key}' localmente");
            let db_dest = base_path.join("lumineria_database");
            mirror_dir(local_database_dir, &db_dest).await?;

            let db_json_src = db_dest.join("modpacks.json");
            if db_json_src.exists() {
                fs::copy(&db_json_src, web_root.join("modpacks.json"))
                    .await
                    .context("no pude actualizar modpacks.json en el web root")?;
            }

            let pack_dest = base_path.join("lumineria_packwiz").join(pack_key);
            if pack_dest.exists() {
                fs::remove_dir_all(&pack_dest)
                    .await
                    .context("no pude borrar el pack de lumineria_packwiz")?;
            }
            let pack_web_dest = web_root.join(pack_key);
            if pack_web_dest.exists() {
                fs::remove_dir_all(&pack_web_dest)
                    .await
                    .context("no pude borrar el pack del web root")?;
            }
        }
    }
    Ok(())
}

/// Copia recursiva simple (no sigue symlinks, suficiente para packs/mods/imágenes).
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).await?;
    let mut entries = fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            Box::pin(copy_dir_recursive(&path, &dest_path)).await?;
        } else {
            fs::copy(&path, &dest_path).await?;
        }
    }
    Ok(())
}

/// Equivalente a `rsync --delete`: borra el destino y lo recrea desde cero.
async fn mirror_dir(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst).await?;
    }
    copy_dir_recursive(src, dst).await
}

#[cfg(unix)]
async fn set_permissions_recursive(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut entries = fs::read_dir(path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let p = entry.path();
        let perms = std::fs::Permissions::from_mode(mode);
        let _ = fs::set_permissions(&p, perms).await;
        if p.is_dir() {
            let _ = Box::pin(set_permissions_recursive(&p, mode)).await;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
async fn set_permissions_recursive(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

async fn rsync(local: &Path, remote_path: &str, ssh_host: Option<&str>) -> Result<()> {
    let local_str = format!("{}/", local.display().to_string().trim_end_matches('/'));
    let dest = match ssh_host {
        Some(host) => format!("{host}:{remote_path}"),
        None => remote_path.to_string(),
    };
    let status = Command::new("rsync")
        .args(["-avz", "--delete", "--mkpath", &local_str, &dest])
        .status()
        .await?;
    if !status.success() {
        bail!("rsync falló hacia {dest}");
    }
    Ok(())
}

async fn run_remote(cmd: &str, ssh_host: &str) -> Result<()> {
    let status = Command::new("ssh").arg(ssh_host).arg(cmd).status().await?;
    if !status.success() {
        bail!("comando remoto falló: {cmd}");
    }
    Ok(())
}
