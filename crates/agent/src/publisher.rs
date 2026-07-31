use anyhow::{bail, Result};
use std::path::Path;
use tokio::process::Command;

pub struct PublishTarget {
    pub ssh_host: String,    // Ej: "TheKaramelito@158.247.125.204"
    pub remote_base: String, // Ej: "~/lumineria"
}

pub async fn publish_packwiz(
    target: &PublishTarget,
    local_pack_dir: &Path,     // Carpeta packwiz del servidor local
    local_database_dir: &Path, // Carpeta lumineria_database local
    pack_key: &str,
) -> Result<()> {
    tracing::info!("Iniciando publicación del modpack '{}'", pack_key);

    // 1. Sincronizar el modpack hacia lumineria_packwiz/<clave>
    let remote_pack = format!("{}/lumineria_packwiz/{}/", target.remote_base, pack_key);
    tracing::info!("Subiendo archivos a {}", remote_pack);
    rsync_dir(local_pack_dir, &remote_pack, target).await?;

    // 2. Sincronizar la base de datos (modpacks.json + imágenes)
    let remote_db = format!("{}/lumineria_database/", target.remote_base);
    tracing::info!("Subiendo base de datos a {}", remote_db);
    rsync_dir(local_database_dir, &remote_db, target).await?;

    // 3. Ejecutar el script publish.sh en el VPS
    tracing::info!("Ejecutando script de Nginx remoto...");
    let run_cmd = format!("bash {}/publish.sh", target.remote_base);
    run_remote(target, &run_cmd).await?;

    Ok(())
}

async fn rsync_dir(local: &Path, remote_path: &str, target: &PublishTarget) -> Result<()> {
    // Nos aseguramos de que el path local termine en "/" para que rsync copie el contenido, no la carpeta padre
    let local_str = format!("{}/", local.display().to_string().trim_end_matches('/'));
    let dest = format!("{}:{}", target.ssh_host, remote_path);
    
    // -a: archive mode (recursivo, mantiene permisos), -v: verbose, -z: compresión, --delete: borra archivos viejos en destino
    let status = Command::new("rsync")
        .args(["-avz", "--delete", &local_str, &dest])
        .status()
        .await?;

    if !status.success() {
        bail!("Rsync falló hacia {}. Verifica tu conexión SSH.", dest);
    }
    Ok(())
}

async fn run_remote(target: &PublishTarget, cmd: &str) -> Result<()> {
    let status = Command::new("ssh")
        .arg(&target.ssh_host)
        .arg(cmd)
        .status()
        .await?;

    if !status.success() {
        bail!("Fallo al ejecutar el comando remoto: {}", cmd);
    }
    Ok(())
}