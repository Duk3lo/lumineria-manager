use anyhow::Result;
use protocol::ServerConfigParams;
use std::path::Path;
use tokio::fs;

pub async fn write_server_env(dest_dir: &Path, config: &ServerConfigParams) -> Result<()> {
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
"#,
        config.display_name,
        config.server_type,
        config.server_type,
        config.port,
        config.min_ram,
        config.max_ram,
        config.online_mode,
        config.enforce_secure_profile,
        config.mc_version
    );

    let env_path = dest_dir.join("server.env");
    fs::write(env_path, env_content).await?;
    Ok(())
}

pub async fn write_server_properties(dest_dir: &Path, config: &ServerConfigParams) -> Result<()> {
    let props_content = format!(
        r#"server-port={}
query.port={}
online-mode={}
enforce-secure-profile={}
motd={}
max-players=20
prevent-proxy-connections=false
"#,
        config.port,
        config.port,
        config.online_mode,
        config.enforce_secure_profile,
        config.display_name
    );

    let props_path = dest_dir.join("server.properties");
    fs::write(props_path, props_content).await?;
    Ok(())
}
