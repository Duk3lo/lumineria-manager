use anyhow::{bail, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn container_action(action: &str, container_id: &str) -> Result<()> {
    let status = Command::new("podman")
        .arg(action)
        .arg(container_id)
        .status()
        .await?;
    if !status.success() {
        bail!(
            "podman {action} {container_id} falló (código {:?})",
            status.code()
        );
    }
    Ok(())
}

pub async fn run_stack_script(root: &Path, script: &str) -> Result<()> {
    let script_path = root.join(script);
    if !script_path.exists() {
        bail!("no encontré {}", script_path.display());
    }

    let status = Command::new("bash")
        .arg(&script_path)
        .current_dir(root)
        .status()
        .await?;

    if !status.success() {
        bail!("{script} falló (código {:?})", status.code());
    }
    Ok(())
}
pub async fn sync_mods_now(container_id: &str) -> Result<()> {
    container_action("restart", container_id).await
}

pub async fn stream_logs(container_id: String, tx: mpsc::UnboundedSender<String>) -> Result<()> {
    let mut child = Command::new("podman")
        .args(["logs", "-f", "--tail", "100", &container_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let mut reader = BufReader::new(stdout).lines();

    while let Some(line) = reader.next_line().await? {
        if tx.send(line).is_err() {
            let _ = child.kill().await;
            break;
        }
    }

    Ok(())
}

pub async fn ensure_network() -> Result<()> {
    let exists = Command::new("podman")
        .args(["network", "exists", "lumineria-net"])
        .status()
        .await?;
    if !exists.success() {
        let status = Command::new("podman")
            .args(["network", "create", "lumineria-net"])
            .status()
            .await?;
        if !status.success() {
            bail!("no pude crear la red podman 'lumineria-net'");
        }
    }
    Ok(())
}

pub fn java_image_for(server_type: &str, mc_version: &str) -> &'static str {
    if server_type == "velocity" {
        return "docker.io/library/eclipse-temurin:25-jre";
    }
    let mut parts = mc_version.split('.');
    let _major = parts.next();
    let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(21);
    let patch: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);

    if minor < 17 {
        "docker.io/library/eclipse-temurin:8-jre"
    } else if minor < 20 || (minor == 20 && patch < 5) {
        "docker.io/library/eclipse-temurin:17-jre"
    } else {
        "docker.io/library/eclipse-temurin:21-jre"
    }
}

pub async fn create_container(id: &str, dest_dir: &Path, port: u16, image: &str) -> Result<()> {
    ensure_network().await?;

    // idempotente: si ya existía, lo tiramos y lo recreamos
    let _ = Command::new("podman").args(["rm", "-f", id]).status().await;

    let volume = format!("{}:/data:Z", dest_dir.display());
    let port_map = format!("{0}:{0}/tcp", port);

    let status = Command::new("podman")
        .args([
            "create",
            "--name", id,
            "--network", "lumineria-net",
            "-v", &volume,
            "-p", &port_map,
            "--restart", "unless-stopped",
            image,
            "sh", "/data/start.sh",
        ])
        .status()
        .await?;

    if !status.success() {
        bail!("podman create falló para {id} (código {:?})", status.code());
    }
    Ok(())
}