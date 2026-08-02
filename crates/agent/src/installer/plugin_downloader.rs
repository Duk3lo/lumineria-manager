use anyhow::{Context, Result};
use protocol::ServerEvent;
use serde_json::Value;
use std::path::Path;
use tokio::fs;
use tokio::sync::mpsc;

const UA: &str = "LumineriaManager/2.0 (contacto: admin@lumineria.local)";

/// Servidores que usan este sistema de descarga directa (sin Packwiz).
/// Paper/Folia lo tienen DISPONIBLE junto con Packwiz; Velocity lo usa
/// como único método (no tiene Packwiz).
pub fn uses_direct_plugins(server_type: &str) -> bool {
    matches!(server_type.to_lowercase().as_str(), "paper" | "folia" | "spigot" | "velocity")
}

fn modrinth_loaders_param(server_type: &str) -> &'static str {
    if server_type.eq_ignore_ascii_case("velocity") {
        "%5B%22velocity%22%5D"
    } else {
        "%5B%22paper%22%2C%22folia%22%2C%22spigot%22%5D"
    }
}

async fn read_requirement_lines(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path).await else {
        return Vec::new();
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

fn plugin_prefix(filename: &str) -> String {
    let no_ext = filename.trim_end_matches(".jar");
    let mut prefix = String::new();
    for part in no_ext.split('-') {
        if part.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            break;
        }
        if !prefix.is_empty() {
            prefix.push('-');
        }
        prefix.push_str(part);
    }
    if prefix.len() < 3 { no_ext.to_string() } else { prefix }
}

async fn clean_old_plugin(plugins_dir: &Path, keep_filename: &str) {
    let prefix = plugin_prefix(keep_filename).to_lowercase();
    if prefix.len() < 3 {
        return;
    }
    let Ok(mut entries) = fs::read_dir(plugins_dir).await else { return };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if name.eq_ignore_ascii_case(keep_filename) {
            continue;
        }
        if name.to_lowercase().starts_with(&prefix) {
            let _ = fs::remove_file(&path).await;
        }
    }
}

async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    let resp = client
        .get(url)
        .header("User-Agent", UA)
        .send()
        .await
        .context("error de red descargando el plugin")?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} al descargar {url}", resp.status());
    }
    let bytes = resp.bytes().await.context("error leyendo el contenido descargado")?;
    fs::write(dest, &bytes).await?;
    Ok(())
}

async fn sync_modrinth_plugin(
    client: &reqwest::Client,
    slug: &str,
    loaders: &str,
    mc_version_hint: &Option<String>,
    plugins_dir: &Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let mut url = format!("https://api.modrinth.com/v2/project/{slug}/version?loaders={loaders}");
    if let Some(v) = mc_version_hint.as_deref().filter(|v| !v.is_empty()) {
        url = format!("{url}&game_versions=%5B%22{v}%22%5D");
    }

    let mut versions: Vec<Value> = match client.get(&url).header("User-Agent", UA).send().await {
        Ok(r) => r.json::<Value>().await
            .unwrap_or(Value::Array(vec![]))
            .as_array()
            .cloned()
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    if versions.is_empty() && mc_version_hint.as_deref().unwrap_or("").len() > 0 {
        let fallback = format!("https://api.modrinth.com/v2/project/{slug}/version?loaders={loaders}");
        versions = match client.get(&fallback).header("User-Agent", UA).send().await {
            Ok(r) => r.json::<Value>().await
                .unwrap_or(Value::Array(vec![]))
                .as_array()
                .cloned()
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
    }

    if versions.is_empty() {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.to_string(),
            line: format!("  [!] Modrinth -> No se encontró ninguna versión de '{slug}' compatible."),
        });
        return;
    }

    let chosen = versions.iter()
        .find(|v| v["version_type"].as_str() == Some("release"))
        .unwrap_or(&versions[0]);

    let file = chosen["files"].as_array()
        .and_then(|files| files.iter().find(|f| f["primary"].as_bool() == Some(true)).or_else(|| files.first()));

    let Some(file) = file else {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.to_string(),
            line: format!("  [!] Modrinth -> No se pudo resolver el archivo de '{slug}'."),
        });
        return;
    };

    let (Some(url), Some(name)) = (file["url"].as_str(), file["filename"].as_str()) else { return };
    let dest = plugins_dir.join(name);
    if dest.exists() {
        return;
    }

    clean_old_plugin(plugins_dir, name).await;
    let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("> Modrinth -> Descargando {name}...") });
    if let Err(e) = download_to(client, url, &dest).await {
        let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("  [!] Modrinth -> Falló {name}: {e}") });
    }
}

async fn sync_github_plugin(
    client: &reqwest::Client,
    repo: &str,
    plugins_dir: &Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let data: Value = match client.get(&api_url).header("User-Agent", UA)
        .header("Accept", "application/vnd.github+json").send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(_) => Value::Null,
    };

    let asset = data["assets"].as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str().unwrap_or("").ends_with(".jar")));

    let Some(asset) = asset else {
        let _ = tx.send(ServerEvent::PackwizLog {
            id: id.to_string(),
            line: format!("  [!] GitHub -> No se encontró un .jar en la última release de '{repo}' (¿límite de API excedido?)."),
        });
        return;
    };

    let (Some(url), Some(name)) = (asset["browser_download_url"].as_str(), asset["name"].as_str()) else { return };
    let dest = plugins_dir.join(name);

    clean_old_plugin(plugins_dir, name).await;
    let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("> GitHub -> Descargando {name}...") });
    if let Err(e) = download_to(client, url, &dest).await {
        let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("  [!] GitHub -> Falló {name}: {e}") });
    }
}

async fn sync_direct_plugin(
    client: &reqwest::Client,
    url: &str,
    plugins_dir: &Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let name = url.split('/').last().unwrap_or("plugin.jar").split('?').next().unwrap_or("plugin.jar").to_string();
    let name = if name.ends_with(".jar") { name } else { format!("{name}.jar") };
    let dest = plugins_dir.join(&name);

    let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("> Directo -> Comprobando {name}...") });
    if let Err(e) = download_to(client, url, &dest).await {
        let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: format!("  [!] Directo -> Falló {name}: {e}") });
    }
}

pub async fn sync_plugins(
    server_type: &str,
    dest_dir: &Path,
    id: &str,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) -> Result<()> {
    let req_dir = dest_dir.join("requirements");
    let plugins_dir = dest_dir.join("plugins");
    fs::create_dir_all(&plugins_dir).await?;

    let env_data = fs::read_to_string(dest_dir.join("server.env")).await.unwrap_or_default();
    // Primero el hint manual (VELOCITY_PLUGIN_MC_VERSION); si no está, para
    // Paper/Folia usamos directamente la versión real del servidor.
    let mc_version_hint = env_data.lines()
        .find_map(|l| l.strip_prefix("VELOCITY_PLUGIN_MC_VERSION="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            env_data.lines()
                .find_map(|l| l.strip_prefix("MC_VERSION="))
                .map(|v| v.trim().trim_matches('"').to_string())
                .filter(|v| !v.is_empty())
        });

    let loaders = modrinth_loaders_param(server_type);

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("no pude construir el cliente HTTP")?;

    let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: "> Sincronizando plugins...".into() });

    for slug in read_requirement_lines(&req_dir.join("modrinth.txt")).await {
        sync_modrinth_plugin(&client, &slug, loaders, &mc_version_hint, &plugins_dir, id, tx).await;
    }
    for repo in read_requirement_lines(&req_dir.join("github.txt")).await {
        sync_github_plugin(&client, &repo, &plugins_dir, id, tx).await;
    }
    for url in read_requirement_lines(&req_dir.join("direct_links.txt")).await {
        sync_direct_plugin(&client, &url, &plugins_dir, id, tx).await;
    }

    let _ = tx.send(ServerEvent::PackwizLog { id: id.to_string(), line: "✅ Sincronización de plugins completada.".into() });
    Ok(())
}