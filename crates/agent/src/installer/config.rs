use anyhow::Result;
use protocol::ServerConfigParams;
use std::net::TcpListener;
use std::path::Path;
use tokio::fs;

pub struct RconCredentials {
    pub port: u16,
    pub password: String,
}

pub fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

pub fn find_free_rcon_port(start_port: u16) -> u16 {
    let mut port = start_port;
    while port < 65535 {
        if is_port_free(port) {
            return port;
        }
        port += 1;
    }
    start_port
}

pub fn rcon_credentials_for(config: &ServerConfigParams) -> RconCredentials {
    let base_rcon_port = config.port.saturating_add(10_000);
    let port = find_free_rcon_port(base_rcon_port);
    RconCredentials {
        port,
        password: generate_password(),
    }
}

fn generate_password() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
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
rcon.ip=127.0.0.1
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
