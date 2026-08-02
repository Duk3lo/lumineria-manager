use anyhow::{Context, Result};
use protocol::{ServerInfo, ServerStatus};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub fn discover(root: &Path) -> Result<Vec<ServerInfo>> {
    let mut out = Vec::new();

    for entry in
        std::fs::read_dir(root).with_context(|| format!("no pude leer {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let env_path = path.join("server.env");
        if !env_path.exists() {
            continue;
        }

        let folder_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let env = parse_env_file(&env_path)?;
        let container_id = sanitize_container_name(&folder_name);
        let status = container_status(&container_id);

        out.push(ServerInfo {
            id: container_id,
            display_name: env.get("SERVER_NAME").cloned().unwrap_or(folder_name),
            server_type: env.get("SERVER_TYPE").cloned().unwrap_or_default(),
            port: env
                .get("SERVER_PORT")
                .and_then(|p| p.parse().ok())
                .unwrap_or(25565),
            mc_version: env
                .get("MC_VERSION")
                .or_else(|| env.get("SERVER_MC_VERSION"))
                .cloned()
                .unwrap_or_default(),
            mod_source: env
                .get("MOD_SOURCE")
                .cloned()
                .unwrap_or_else(|| "requirements".into()),
            status,
        });
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)?;
    let mut map = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        map.insert(key.trim().to_string(), value.to_string());
    }

    Ok(map)
}

pub fn sanitize_container_name(folder: &str) -> String {
    let cleaned: String = folder
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn container_status(container_id: &str) -> ServerStatus {
    let output = Command::new("podman")
        .args(["inspect", "-f", "{{.State.Status}}", container_id])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            match s.as_str() {
                "running" => ServerStatus::Running,
                "stopping" | "restarting" => ServerStatus::Restarting,
                "exited" | "created" | "stopped" | "configured" | "paused" | "removing" => {
                    ServerStatus::Stopped
                }
                _ => ServerStatus::Unknown,
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
            if stderr.contains("no such container") || stderr.contains("no container with name") {
                ServerStatus::Missing
            } else {
                ServerStatus::Unknown
            }
        }
        _ => ServerStatus::Unknown,
    }
}

pub fn is_port_registered(root: &Path, port: u16) -> Result<bool> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let env_path = path.join("server.env");
        if !env_path.exists() {
            continue;
        }
        let env = parse_env_file(&env_path)?;
        if let Some(p) = env.get("SERVER_PORT").and_then(|p| p.parse::<u16>().ok()) {
            if p == port {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
