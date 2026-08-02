mod api;
mod docker;
mod installer;
mod publisher;
mod system;

use clap::{Parser, Subcommand};
use std::path::Path;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "lumineria-agent",
    about = "Agente de control para el stack de Lumineria"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    CheckDeps {
        #[arg(long)]
        install: bool,
    },
    Serve {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        #[arg(long)]
        vps_ssh_host: Option<String>,
        #[arg(long, default_value = "~/lumineria")]
        vps_remote_base: String,
        #[arg(long, default_value = "localhost")]
        domain: String,
        #[arg(long)]
        token: Option<String>,
    },
}

fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
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

fn is_raw_ip_or_localhost(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    let host_without_port = host.split(':').next().unwrap_or(host);

    host_without_port.parse::<std::net::IpAddr>().is_ok()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::CheckDeps { install } => system::deps::check_and_maybe_install(install).await?,

        Command::Serve {
            root,
            bind,
            vps_ssh_host,
            vps_remote_base,
            domain,
            token,
        } => {
            let root = root.canonicalize()?;
            let domain = normalize_base_url(&domain);
            let token = token
                .or_else(|| std::env::var("LUMINERIA_TOKEN").ok())
                .unwrap_or_else(generate_token);
            tracing::warn!("🔑 Token del agente: {token}");
            tracing::warn!("   Conectá con: ws://host:puerto/ws?token={token}");
            let publish_target = match vps_ssh_host {
                Some(host) => publisher::PublishTarget::Ssh {
                    ssh_host: host,
                    remote_base: vps_remote_base,
                    web_root: "/var/www/html".to_string(),
                },
                None => publisher::PublishTarget::LocalFs {
                    base_path: expand_tilde(&vps_remote_base),
                    web_root: PathBuf::from("/var/www/html"),
                },
            };
            api::ws::serve(root, bind, publish_target, domain, token).await?;
        }
    }

    Ok(())
}

fn build_file_tree<'a>(
    base_dir: &'a Path,
    rel_path: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<protocol::FileNode>> + Send + 'a>> {
    Box::pin(async move {
        let mut nodes = Vec::new();
        let target = if rel_path.is_empty() {
            base_dir.to_path_buf()
        } else {
            base_dir.join(rel_path)
        };

        if let Ok(mut entries) = tokio::fs::read_dir(target).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();

                // Ocultamos la carpeta interna de git y el archivo temporal de packwiz si existen
                if name == ".git" || name == "packwiz.exe" {
                    continue;
                }

                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                let new_rel = if rel_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", rel_path, name)
                };

                let children = if is_dir {
                    Some(build_file_tree(base_dir, &new_rel).await)
                } else {
                    None
                };

                nodes.push(protocol::FileNode {
                    name,
                    is_dir,
                    path: new_rel,
                    children,
                });
            }
        }

        // Ordenar: Carpetas primero, luego orden alfabético
        nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        nodes
    })
}
