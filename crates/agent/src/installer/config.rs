use anyhow::Result;
use protocol::ServerConfigParams;
use std::path::Path;

pub async fn write_server_env(
    dest_dir: &Path,
    config: &ServerConfigParams,
) -> Result<()> {
    // 10 variables, 10 argumentos en total. Nada de RCON.
    let env_content = format!(
        r#"# === IDENTIFICACIÓN ===
SERVER_NAME="{}"
SERVER_TYPE="{}"
SERVER_BUILD_MODE="{}"
SERVER_MOTD="{}"

# === RED Y RENDIMIENTO ===
SERVER_PORT="{}"
MIN_RAM="{}"
MAX_RAM="{}"
ONLINE_MODE="{}"
ENFORCE_SECURE_PROFILE="{}"

# === MINECRAFT ===
MC_VERSION="{}"
"#,
        config.display_name,
        config.server_type,
        config.server_type,
        config.display_name, // Usamos el nombre del server como MOTD por defecto
        config.port,
        config.min_ram,
        config.max_ram,
        config.online_mode,
        config.enforce_secure_profile,
        config.mc_version,
    );

    let env_path = dest_dir.join("server.env");
    tokio::fs::write(env_path, env_content).await?;
    Ok(())
}

pub async fn write_server_properties(
    dest_dir: &Path,
    config: &ServerConfigParams,
) -> Result<()> {
    // RCON desactivado por defecto (enable-rcon=false)
    let props_content = format!(
        r#"server-port={}
query.port={}
online-mode={}
enforce-secure-profile={}
motd={}
max-players=20
prevent-proxy-connections=false
enable-rcon=false
"#,
        config.port,
        config.port,
        config.online_mode,
        config.enforce_secure_profile,
        config.display_name
    );

    let props_path = dest_dir.join("server.properties");
    tokio::fs::write(props_path, props_content).await?;
    Ok(())
}

pub async fn update_env_key(
    dest_dir: &std::path::Path,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    let env_path = dest_dir.join("server.env");
    let content = tokio::fs::read_to_string(&env_path)
        .await
        .unwrap_or_default();

    let mut found = false;
    let mut new_lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{key}=")) {
                found = true;
                format!("{key}=\"{value}\"")
            } else {
                line.to_string()
            }
        })
        .collect();

    if !found {
        new_lines.push(format!("{key}=\"{value}\""));
    }

    tokio::fs::write(&env_path, new_lines.join("\n") + "\n").await?;
    Ok(())
}

pub async fn patch_velocity_toml(
    dest_dir: &std::path::Path,
    port: u16,
    motd: Option<&str>,
) -> anyhow::Result<()> {
    let toml_path = dest_dir.join("velocity.toml");
    if !toml_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&toml_path).await?;
    let mut found_bind = false;
    let mut found_motd = false;

    let mut new_lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("bind") && trimmed.contains('=') {
                found_bind = true;
                format!("bind = \"0.0.0.0:{port}\"")
            } else if let Some(m) = motd {
                if trimmed.starts_with("motd") && trimmed.contains('=') {
                    found_motd = true;
                    format!("motd = \"{}\"", m.replace('"', "\\\""))
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect();

    if !found_bind {
        new_lines.insert(0, format!("bind = \"0.0.0.0:{port}\""));
    }
    if let Some(m) = motd {
        if !found_motd {
            new_lines.push(format!("motd = \"{}\"", m.replace('"', "\\\"")));
        }
    }

    tokio::fs::write(&toml_path, new_lines.join("\n") + "\n").await?;
    Ok(())
}