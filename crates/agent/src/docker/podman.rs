use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn container_action(action: &str, container_id: &str) -> Result<()> {
    let mut cmd = Command::new("podman");
    cmd.arg(action);
    if action == "stop" || action == "restart" {
        cmd.args(["-t", "60"]);
    }
    cmd.arg(container_id);

    let status = cmd.status().await?;
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
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let tx_out = tx.clone();
    let out_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if tx_out.send(line).is_err() {
                break;
            }
        }
    });

    let tx_err = tx.clone();
    let err_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if tx_err.send(line).is_err() {
                break;
            }
        }
    });

    let _ = child.wait().await;
    let _ = out_task.await;
    let _ = err_task.await;

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

pub async fn create_container(id: &str, dest_dir: &Path, image: &str) -> Result<()> {
    let _ = Command::new("podman").args(["rm", "-f", id]).status().await;

    let volume = format!("{}:/data:Z", dest_dir.display());

    let status = Command::new("podman")
        .args([
            "create",
            "--name", id,
            "--network", "host",
            "--userns=keep-id",
            "-i",
            "-v", &volume,
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

pub async fn send_stdin_command(container_id: &str, command: &str) -> Result<()> {
    let mut child = Command::new("podman")
        .args(["exec", "-i", container_id, "sh", "-c", "cat > /proc/1/fd/0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("no pude ejecutar podman exec")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .context("no pude escribir el comando")?;
        stdin.shutdown().await.ok();
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("podman exec falló: {}", err.trim());
    }
    Ok(())
}


pub async fn delete_container(container_id: &str) -> Result<()> {
    let _ = Command::new("podman")
        .args(["stop", "-t", "5", container_id])
        .status()
        .await;

    let output = Command::new("podman")
        .args(["rm", "-f", "-t", "0", container_id])
        .output()
        .await?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if !err.to_lowercase().contains("no such container")
            && !err.to_lowercase().contains("no container with name")
        {
            bail!("Error interno de Podman: {}", err.trim());
        }
    }
    Ok(())
}

pub async fn is_running(container_id: &str) -> bool {
    let output = Command::new("podman")
        .args(["inspect", "-f", "{{.State.Running}}", container_id])
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim() == "true",
        _ => false,
    }
}
