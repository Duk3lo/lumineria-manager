use anyhow::{bail, Result};
use protocol::ServerEvent;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;

const PAPER_API_BASE: &str = "https://fill.papermc.io/v3/projects";
const UA: &str = "LumineriaManager/2.0 (contacto: admin@lumineria.local)";

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    server_id: &str,
    step_name: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> Result<()> {
    let response = client.get(url).header("User-Agent", UA).send().await?;
    if !response.status().is_success() {
        bail!("Fallo HTTP: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut file = fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
        let chunk = item?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percentage = ((downloaded as f32 / total_size as f32) * 100.0) as u8;
            let _ = tx.send(ServerEvent::InstallProgress {
                id: server_id.to_string(),
                step: step_name.to_string(),
                percentage,
            });
        }
    }
    Ok(())
}

pub async fn install_papermc(
    client: &reqwest::Client,
    project: &str,
    mc_version: &str,
    dest_dir: &Path,
    server_id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    min_ram: &str,
    max_ram: &str,
) -> Result<String> {
    let version_url = format!("{}/{}/versions/{}", PAPER_API_BASE, project, mc_version);
    let builds_res = client
        .get(&format!("{}/builds", version_url))
        .header("User-Agent", UA)
        .send()
        .await?;

    if !builds_res.status().is_success() {
        bail!("La versión {} no es válida para {}", mc_version, project);
    }

    let builds: Value = builds_res.json().await?;
    let arr = builds
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Respuesta inválida de builds"))?;

    let best_build = arr
        .iter()
        .find(|b| {
            let chan = b["channel"].as_str().unwrap_or("");
            chan == "STABLE" || chan == "RECOMMENDED"
        })
        .or_else(|| arr.first())
        .ok_or_else(|| anyhow::anyhow!("No hay compilaciones disponibles"))?;

    let jar_name = best_build["downloads"]["server:default"]["name"]
        .as_str()
        .unwrap_or("server.jar")
        .to_string();
    let download_url = best_build["downloads"]["server:default"]["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No se encontró la url de descarga"))?;

    let output_path = dest_dir.join(&jar_name);
    download_file(
        client,
        download_url,
        &output_path,
        server_id,
        "Descargando Motor",
        tx,
    )
    .await?;

    write_start_script(dest_dir, project, min_ram, max_ram, &jar_name).await?;

    Ok(jar_name)
}

pub async fn install_fabric(
    client: &reqwest::Client,
    mc_version: &str,
    loader_version: &str,
    dest_dir: &Path,
    server_id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    min_ram: &str,
    max_ram: &str,
) -> Result<()> {
    let response = client
        .get("https://meta.fabricmc.net/v2/versions/installer")
        .send()
        .await?;
    let installers: serde_json::Value = response.json().await?;
    let installer_ver = installers
        .as_array()
        .and_then(|arr| arr.iter().find(|i| i["stable"].as_bool() == Some(true)))
        .and_then(|i| i["version"].as_str())
        .unwrap_or("1.0.1");

    let installer_url = format!(
        "https://maven.fabricmc.net/net/fabricmc/fabric-installer/{0}/fabric-installer-{0}.jar",
        installer_ver
    );

    let installer_path = dest_dir.join("fabric-installer.jar");

    download_file(
        client,
        &installer_url,
        &installer_path,
        server_id,
        "Descargando Fabric Installer",
        tx,
    )
    .await?;

    let _ = tx.send(ServerEvent::InstallProgress {
        id: server_id.to_string(),
        step: "Ejecutando instalador de Fabric y descargando Minecraft...".to_string(),
        percentage: 60,
    });

    let mut child = Command::new("java")
        .arg("-jar")
        .arg(&installer_path)
        .arg("server")
        .arg("-mcVersion")
        .arg(mc_version)
        .arg("-loader")
        .arg(loader_version)
        .arg("-downloadMinecraft")
        .current_dir(dest_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;

    if !status.success() {
        bail!("Fallo en la ejecución de fabric-installer.");
    }

    let _ = fs::remove_file(installer_path).await;
    write_start_script(
        dest_dir,
        "fabric",
        min_ram,
        max_ram,
        "fabric-server-launch.jar",
    )
    .await?;
    Ok(())
}

pub async fn install_mod_installer(
    client: &reqwest::Client,
    url: &str,
    installer_name: &str,
    dest_dir: &Path,
    server_id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    min_ram: &str,
    max_ram: &str,
) -> Result<()> {
    let installer_path = dest_dir.join(installer_name);
    download_file(
        client,
        url,
        &installer_path,
        server_id,
        "Descargando Instalador",
        tx,
    )
    .await?;

    let _ = tx.send(ServerEvent::InstallProgress {
        id: server_id.to_string(),
        step: "Extrayendo librerías de Minecraft... (Esto tomará un momento)".to_string(),
        percentage: 50,
    });

    let mut child = Command::new("java")
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installServer")
        .current_dir(dest_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;
    if !status.success() {
        bail!("Fallo en la instalación del cargador de mods.");
    }

    let _ = fs::remove_file(installer_path).await;

    if dest_dir.join("run.sh").exists() {
        write_start_script_run_sh(dest_dir).await?;
    } else {
        let jar = find_launch_jar(dest_dir).await?;
        write_start_script(dest_dir, "forge", min_ram, max_ram, &jar).await?;
    }

    Ok(())
}

async fn find_launch_jar(dest_dir: &Path) -> Result<String> {
    let mut entries = fs::read_dir(dest_dir).await?;
    let mut best: Option<String> = None;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".jar") && !name.to_lowercase().contains("installer") {
            best = Some(name);
        }
    }
    best.ok_or_else(|| anyhow::anyhow!("No encontré un jar ejecutable tras la instalación"))
}

async fn write_launch_script(dest_dir: &Path, content: &str) -> Result<()> {
    let path = dest_dir.join("start.sh");
    fs::write(&path, content).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).await?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).await?;
    }
    Ok(())
}

pub async fn write_start_script(
    dest_dir: &Path,
    server_type: &str,
    min_ram: &str,
    max_ram: &str,
    jar_name: &str,
) -> Result<()> {
    let launch = if server_type == "velocity" {
        format!("java -Xms{min_ram} -Xmx{max_ram} -jar {jar_name}")
    } else {
        format!("java -Xms{min_ram} -Xmx{max_ram} -jar {jar_name} nogui")
    };
    write_launch_script(dest_dir, &format!("#!/bin/sh\ncd /data\nexec {launch}\n")).await
}

pub async fn write_start_script_run_sh(dest_dir: &Path) -> Result<()> {
    write_launch_script(dest_dir, "#!/bin/sh\ncd /data\nexec sh run.sh nogui\n").await
}
