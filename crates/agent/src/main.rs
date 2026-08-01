mod api;
mod docker;
mod installer;
mod publisher;
mod system;
mod rcon;

use clap::{Parser, Subcommand};
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
    },
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
    } else {
        format!("https://{trimmed}")
    }
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
        } => {
            let root = root.canonicalize()?;
            let domain = normalize_base_url(&domain);
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
            api::ws::serve(root, bind, publish_target, domain).await?;
        }
    }

    Ok(())
}
