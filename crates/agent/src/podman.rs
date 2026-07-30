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
