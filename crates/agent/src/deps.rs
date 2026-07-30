//! Revisa que existan los comandos que el stack necesita (los mismos que
//! ya chequeaba `check_dependencies()` en lib_core.sh: jq, curl, unzip,
//! python3 — acá se agrega podman porque el agente también lo necesita)
//! y, si el usuario lo pide explícitamente, los instala.
//!
//! Reglas de "instalación segura":
//!   - Nunca `curl | bash`. Solo el gestor de paquetes oficial de la distro.
//!   - Nunca se corre como root a ciegas: siempre vía `sudo` explícito.
//!   - Nunca se instala sin confirmación interactiva del usuario.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

const REQUIRED_COMMANDS: &[(&str, &str)] = &[
    ("podman", "podman"),
    ("jq", "jq"),
    ("curl", "curl"),
    ("python3", "python3"),
    ("unzip", "unzip"),
];

/// Busca `cmd` en cada directorio de $PATH, igual que hace el shell.
/// Evitamos la crate `which` a propósito: en este proyecto arrastraba
/// una dependencia transitiva que exige una edición de Cargo más nueva
/// que la que trae Ubuntu 24.04 — para algo tan simple como "está este
/// binario en el PATH" no vale la pena la fricción de versión.
fn find_in_path(cmd: &str) -> Option<PathBuf> {
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

pub fn check_and_maybe_install(install: bool) -> Result<()> {
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

    if missing.is_empty() {
        println!("Todas las dependencias están presentes.");
        return Ok(());
    }

    println!("Faltan estos paquetes: {}", missing.join(", "));

    if !install {
        println!("Corré de nuevo con --install para instalarlos (se pide confirmación).");
        return Ok(());
    }

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

    let pm = detect_package_manager()?;
    install_packages(pm, &missing)?;
    Ok(())
}

fn install_packages(pm: PackageManager, packages: &[&str]) -> Result<()> {
    if matches!(pm, PackageManager::Apt) {
        let status = Command::new("sudo")
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

    tracing::info!("Ejecutando: sudo {}", args.join(" "));
    let status = Command::new("sudo")
        .args(&args)
        .status()
        .context("no pude invocar sudo")?;

    if !status.success() {
        bail!("La instalación falló (código {:?})", status.code());
    }

    println!("Paquetes instalados correctamente.");
    Ok(())
}
