use anyhow::{bail, Context, Result};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command as SyncCommand;
use tokio::process::Command;

const REQUIRED_COMMANDS: &[(&str, &str)] = &[
    ("podman", "podman"),
    ("jq", "jq"),
    ("curl", "curl"),
    ("python3", "python3"),
    ("unzip", "unzip"),
    ("rsync", "rsync"),
    ("ssh", "openssh-client"),
    ("go", "golang"),
];

pub fn find_in_path(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(cmd);
        candidate.is_file().then_some(candidate)
    })
}

#[derive(Debug, Clone, Copy)]
enum PackageManager {
    Apt,
    Dnf,
}

fn detect_package_manager() -> Result<PackageManager> {
    if find_in_path("apt-get").is_some() {
        return Ok(PackageManager::Apt);
    }
    if find_in_path("dnf").is_some() {
        return Ok(PackageManager::Dnf);
    }

    let hint = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    bail!(
        "No reconozco un gestor de paquetes soportado (apt/dnf) en esta distro.\n{}",
        hint.lines().next().unwrap_or("")
    )
}

pub async fn check_and_maybe_install(install: bool) -> Result<()> {
    let mut missing = Vec::new();

    for (cmd, pkg) in REQUIRED_COMMANDS {
        match find_in_path(cmd) {
            Some(path) => tracing::info!("✔ {cmd} -> {}", path.display()),
            None => {
                tracing::warn!("✘ {cmd} no encontrado");
                missing.push(*pkg);
            }
        }
    }

    if !missing.is_empty() {
        if !install {
            bail!("Faltan estos paquetes: {}", missing.join(", "));
        }

        let is_tty = std::io::stdin().is_terminal();

        if is_tty {
            print!(
                "¿Instalar {} vía el gestor de paquetes oficial del sistema? [y/N] ",
                missing.join(", ")
            );
            std::io::stdout().flush().ok();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if !answer.trim().eq_ignore_ascii_case("y") {
                println!("Cancelado, no se instaló nada.");
                return Ok(());
            }
        }

        let sudo_cmd = if is_tty { "sudo" } else { "pkexec" };
        let pm = detect_package_manager()?;
        install_packages(pm, sudo_cmd, &missing)?;
    } else {
        println!("Todas las dependencias de sistema están presentes.");
    }

    // Retorna la ruta confirmada y utilizable de packwiz
    let packwiz_path = ensure_packwiz().await?;
    tracing::info!("Binario listo en: {}", packwiz_path.display());

    Ok(())
}

fn install_packages(pm: PackageManager, sudo_cmd: &str, packages: &[&str]) -> Result<()> {
    if matches!(pm, PackageManager::Apt) {
        let status = SyncCommand::new(sudo_cmd)
            .args(["apt-get", "update", "-qq"])
            .status()
            .context("no pude ejecutar apt-get update")?;
        if !status.success() {
            bail!("apt-get update falló");
        }
    }

    let mut args: Vec<String> = match pm {
        PackageManager::Apt => vec!["apt-get".into(), "install".into(), "-y".into()],
        PackageManager::Dnf => vec!["dnf".into(), "install".into(), "-y".into()],
    };
    args.extend(packages.iter().map(|p| p.to_string()));

    tracing::info!("Ejecutando: {} {}", sudo_cmd, args.join(" "));
    let status = SyncCommand::new(sudo_cmd)
        .args(&args)
        .status()
        .context(format!("no pude invocar {}", sudo_cmd))?;

    if !status.success() {
        bail!("La instalación falló (código {:?})", status.code());
    }

    println!("Paquetes instalados correctamente.");
    Ok(())
}

/// Helper para determinar dónde instala Go los binarios en este sistema
fn get_go_bin_dir() -> Result<PathBuf> {
    if let Ok(gobin) = std::env::var("GOBIN") {
        return Ok(PathBuf::from(gobin));
    }
    if let Ok(gopath) = std::env::var("GOPATH") {
        return Ok(PathBuf::from(gopath).join("bin"));
    }
    let home = std::env::var("HOME").context("No se pudo obtener la variable HOME")?;
    Ok(PathBuf::from(home).join("go").join("bin"))
}

/// Usa `go install` para descargar, compilar e instalar Packwiz
pub async fn ensure_packwiz() -> Result<PathBuf> {
    // 1. Revisamos si ya está accesible directamente en el PATH
    if let Some(path) = find_in_path("packwiz") {
        tracing::info!("✔ packwiz -> {}", path.display());
        return Ok(path);
    }

    // 2. Revisamos si el binario ya fue compilado previamente en el directorio de Go
    let go_bin_dir = get_go_bin_dir()?;
    let go_bin = go_bin_dir.join("packwiz");

    if go_bin.is_file() {
        tracing::info!("✔ packwiz -> {}", go_bin.display());
        return Ok(go_bin);
    }

    // 3. Si no existe, lo instalamos usando el módulo oficial actual
    tracing::warn!("packwiz no encontrado. Descargando y compilando vía 'go install'...");

    let status = Command::new("go")
        .args(["install", "github.com/packwiz/packwiz@latest"])
        .status()
        .await
        .context("Fallo al ejecutar el comando 'go'")?;

    if !status.success() {
        bail!("Fallo al instalar packwiz usando 'go install'");
    }

    if go_bin.is_file() {
        tracing::info!("✔ packwiz instalado exitosamente en {:?}", go_bin);
        Ok(go_bin)
    } else {
        bail!(
            "La instalación terminó sin errores de 'go', pero no se encontró el binario en {:?}",
            go_bin
        )
    }
}
