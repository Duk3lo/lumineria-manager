mod deps;
mod podman;
mod servers;
mod ws;
mod installer;
mod config_writer;

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
    CheckDeps {
        #[arg(long)]
        install: bool,
    },
    Serve {
        #[arg(long, default_value = ".")]
        root: PathBuf,
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
