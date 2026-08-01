use anyhow::Result;
use protocol::ServerConfigParams;
use std::path::Path;
use tokio::fs;

pub struct RconCredentials {
    pub port: u16,
    pub password: String,
}

pub fn rcon_credentials_for(config: &ServerConfigParams) -> RconCredentials {
    let port = config.port.saturating_add(10_000);
    RconCredentials {
        port,
        password: generate_password(&config.display_name),
    }
}

fn generate_password(seed: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub async fn write_server_env(
    dest_dir: &Path,
    config: &ServerConfigParams,
    rcon: &RconCredentials,
) -> Result<()> {
    let env_content = format!(
        r#"# === IDENTIFICACIÓN ===
SERVER_NAME="{}"
SERVER_TYPE="{}"
SERVER_BUILD_MODE="{}"

# === RED Y RENDIMIENTO ===
SERVER_PORT="{}"
MIN_RAM="{}"
MAX_RAM="{}"
ONLINE_MODE="{}"
ENFORCE_SECURE_PROFILE="{}"

# === MINECRAFT ===
MC_VERSION="{}"

# === RCON ===
RCON_PORT="{}"
RCON_PASSWORD="{}"
"#,
        config.display_name,
        config.server_type,
        config.server_type,
        config.port,
        config.min_ram,
        config.max_ram,
        config.online_mode,
        config.enforce_secure_profile,
        config.mc_version,
        rcon.port,
        rcon.password
    );

    let env_path = dest_dir.join("server.env");
    fs::write(env_path, env_content).await?;
    Ok(())
}

pub async fn write_server_properties(
    dest_dir: &Path,
    config: &ServerConfigParams,
    rcon: &RconCredentials,
) -> Result<()> {
    let props_content = format!(
        r#"server-port={}
query.port={}
online-mode={}
enforce-secure-profile={}
motd={}
max-players=20
prevent-proxy-connections=false
enable-rcon=true
rcon.port={}
rcon.password={}
"#,
        config.port,
        config.port,
        config.online_mode,
        config.enforce_secure_profile,
        config.display_name,
        rcon.port,
        rcon.password
    );

    let props_path = dest_dir.join("server.properties");
    fs::write(props_path, props_content).await?;
    Ok(())
}