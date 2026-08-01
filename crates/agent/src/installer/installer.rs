use anyhow::{bail, Context, Result};
use protocol::ServerEvent;
use serde_json::Value;
use std::path::{Path, PathBuf};
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
        step: "Ejecutando instalador de Fabric en Podman...".to_string(),
        percentage: 60,
    });

    let vol_data = format!("{}:/data:Z", dest_dir.display());

    let mut child = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--network", "host",
            "--userns=keep-id",
            "-v", &vol_data,
            "-w", "/data",
            "docker.io/library/eclipse-temurin:21-jre",
            "java",
            "-jar",
            "fabric-installer.jar",
            "server",
            "-mcVersion",
            mc_version,
            "-loader",
            loader_version,
            "-downloadMinecraft"
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;

    if !status.success() {
        bail!("Fallo en la ejecución de fabric-installer en Podman.");
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
        step: "Extrayendo librerías en Podman... (Tomará un momento)".to_string(),
        percentage: 50,
    });

    let vol_data = format!("{}:/data:Z", dest_dir.display());

    let mut child = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--network", "host",
            "--userns=keep-id",
            "-v", &vol_data,
            "-w", "/data",
            "docker.io/library/eclipse-temurin:21-jre",
            "java",
            "-jar",
            installer_name,
            "--installServer"
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;
    if !status.success() {
        bail!("Fallo en la instalación del cargador de mods en Podman.");
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

const PACKWIZ_INSTALLER_BOOTSTRAP_URL: &str = "https://github.com/packwiz/packwiz-installer-bootstrap/releases/download/v0.0.3/packwiz-installer-bootstrap.jar";

async fn ensure_installer_bootstrap(client: &reqwest::Client) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("no pude leer $HOME")?;
    let cache_dir = PathBuf::from(home).join(".cache").join("lumineria");
    fs::create_dir_all(&cache_dir).await?;
    let jar_path = cache_dir.join("packwiz-installer-bootstrap.jar");

    if jar_path.is_file() {
        return Ok(jar_path);
    }

    let bytes = client
        .get(PACKWIZ_INSTALLER_BOOTSTRAP_URL)
        .send()
        .await?
        .bytes()
        .await?;
    fs::write(&jar_path, &bytes).await?;
    Ok(jar_path)
}

pub async fn sync_server_mods(
    client: &reqwest::Client,
    pack_toml_url: &str,
    dest_dir: &Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> Result<()> {
    let bootstrap = ensure_installer_bootstrap(client).await?;

    let _ = tx.send(ServerEvent::PackwizLog {
        id: id.to_string(),
        line: "> Descargando mods del servidor (vía Podman)...".into(),
    });

    let vol_data = format!("{}:/data:Z", dest_dir.display());
    // Montamos el jar descargado en una ruta temporal dentro del contenedor
    let vol_jar = format!("{}:/tmp/bootstrap.jar:z,ro", bootstrap.display());

    let output = tokio::process::Command::new("podman")
        .args([
            "run",
            "--rm",               // Se borra al terminar
            "--network", "host",  // Para que tenga internet sin problemas
            "--userns=keep-id",   // Mantiene los permisos de tu usuario Linux
            "-v", &vol_data,
            "-v", &vol_jar,
            "-w", "/data",
            "docker.io/library/eclipse-temurin:21-jre",
            "java",
            "-jar",
            "/tmp/bootstrap.jar",
            "-g",
            "-s",
            "server",
            pack_toml_url,
        ])
        .output()
        .await
        .context("no pude ejecutar podman run para packwiz-installer")?;

    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
    {
        if !line.trim().is_empty() {
            let _ = tx.send(ServerEvent::PackwizLog {
                id: id.to_string(),
                line: line.trim().to_string(),
            });
        }
    }

    if !output.status.success() {
        bail!(
            "packwiz-installer-bootstrap terminó con error (código {:?})",
            output.status.code()
        );
    }

    Ok(())
}
