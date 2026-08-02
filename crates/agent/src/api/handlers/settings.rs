use super::super::state::AppState;
use crate::publisher::PublishTarget;
use protocol::ServerEvent;
use std::path::PathBuf;
use tokio::sync::mpsc;

fn is_raw_ip_or_localhost(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    let host_without_port = host.split(':').next().unwrap_or(host);
    host_without_port.parse::<std::net::IpAddr>().is_ok()
}

fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if is_raw_ip_or_localhost(trimmed) {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub(crate) async fn set_publish_config(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    ssh_host: Option<String>,
    remote_base: String,
    domain: String,
) {
    let ssh_host = ssh_host
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    let remote_base = if remote_base.trim().is_empty() {
        "~/lumineria".to_string()
    } else {
        remote_base.trim().to_string()
    };
    let domain = normalize_base_url(&domain);

    let new_target = match &ssh_host {
        Some(host) => PublishTarget::Ssh {
            ssh_host: host.clone(),
            remote_base: remote_base.clone(),
            web_root: "/var/www/html".to_string(),
        },
        None => PublishTarget::LocalFs {
            base_path: expand_tilde(&remote_base),
            web_root: PathBuf::from("/var/www/html"),
        },
    };

    *state.publish_target.write().await = new_target;
    *state.domain.write().await = domain.clone();

    let _ = tx.send(ServerEvent::Ack {
        ok: true,
        message: Some(format!(
            "✅ Configuración de publicación actualizada. Dominio: {domain}"
        )),
    });
}

pub(crate) async fn get_publish_config(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let domain = state.domain.read().await.clone();
    let (ssh_host, remote_base) = match &*state.publish_target.read().await {
        PublishTarget::Ssh { ssh_host, remote_base, .. } => {
            (Some(ssh_host.clone()), remote_base.clone())
        }
        PublishTarget::LocalFs { base_path, .. } => {
            (None, base_path.to_string_lossy().to_string())
        }
    };

    let _ = tx.send(ServerEvent::PublishConfig { ssh_host, remote_base, domain });
}