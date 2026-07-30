mod deps;
mod podman;
mod servers;
mod ws;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lumineria-agent", about = "Agente de control para el stack de Lumineria")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Revisa que estén podman/jq/curl/python3/unzip y, con --install,
    /// los instala vía el gestor de paquetes oficial de la distro.
    CheckDeps {
        #[arg(long)]
        install: bool,
    },
    /// Levanta el servidor WebSocket que consume el cliente Tauri.
    Serve {
        /// Carpeta raíz del proyecto (donde están start-podman.sh,
        /// stop-podman.sh, scripts/, y las carpetas de cada servidor).
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Dirección de bind. Se recomienda dejarla en loopback y
        /// exponerla solo vía túnel SSH, nunca directo a internet.
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::CheckDeps { install } => deps::check_and_maybe_install(install)?,
        Command::Serve { root, bind } => {
            let root = root.canonicalize()?;
            ws::serve(root, bind).await?;
        }
    }

    Ok(())
}
