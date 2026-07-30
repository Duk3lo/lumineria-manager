//! Capa delgada sobre podman. A propósito NO reimplementa nada de lo que
//! ya hacen tus scripts: para el stack completo (start-podman.sh,
//! stop-podman.sh, restart-podman.sh) simplemente los invoca. Para
//! control por-servidor usa `podman start/stop/restart/logs` directo
//! contra el contenedor individual.

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

/// Ejecuta uno de los scripts que ya tenés en la raíz del proyecto
/// (start-podman.sh, stop-podman.sh, restart-podman.sh) en vez de
/// reimplementar la orquestación del stack completo.
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

/// "Sincronizar mods ahora": como `download_mods_packwiz` ya corre al
/// inicio de cada vuelta del loop en runner.sh, la forma más simple y
/// segura de forzarlo sin duplicar lógica es reiniciar el contenedor.
pub async fn sync_mods_now(container_id: &str) -> Result<()> {
    container_action("restart", container_id).await
}

/// Sigue el log de un contenedor y manda cada línea por el canal dado.
/// Termina cuando el proceso `podman logs` termina, o cuando el otro
/// lado del canal se cierra (el cliente se desuscribió).
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
