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

    // 👇 movido afuera, calculado una sola vez para toda la función
    let is_tty = std::io::stdin().is_terminal();
    let sudo_cmd = if is_tty { "sudo" } else { "pkexec" };

    if !missing.is_empty() {
        if !install {
            bail!("Faltan estos paquetes: {}", missing.join(", "));
        }

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

        let pm = detect_package_manager()?;
        install_packages(pm, sudo_cmd, &missing)?;
    } else {
        println!("Todas las dependencias de sistema están presentes.");
    }

    let packwiz_path = ensure_packwiz().await?;
    tracing::info!("Binario listo en: {}", packwiz_path.display());

    let pm = detect_package_manager()?;
    ensure_web_server(sudo_cmd, pm).await?;

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
pub fn get_go_bin_dir() -> Result<PathBuf> {
    if let Ok(gobin) = std::env::var("GOBIN") {
        return Ok(PathBuf::from(gobin));
    }
    if let Ok(gopath) = std::env::var("GOPATH") {
        return Ok(PathBuf::from(gopath).join("bin"));
    }
    let home = std::env::var("HOME").context("No se pudo obtener la variable HOME")?;
    Ok(PathBuf::from(home).join("go").join("bin"))
}

pub fn resolve_packwiz_bin() -> PathBuf {
    if let Some(path) = find_in_path("packwiz") {
        return path;
    }
    if let Ok(go_bin_dir) = get_go_bin_dir() {
        let candidate = go_bin_dir.join("packwiz");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("packwiz")
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

async fn ensure_web_server(sudo_cmd: &str, pm: PackageManager) -> Result<()> {
    // 1. ¿Ya hay algo activo en el puerto 80? (Apache o Nginx)
    let apache_active = SyncCommand::new("systemctl")
        .args(["is-active", "--quiet", "apache2"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let nginx_active = SyncCommand::new("systemctl")
        .args(["is-active", "--quiet", "nginx"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if apache_active || nginx_active {
        println!(
            "✔ Ya hay un servidor web activo ({}), no instalo nada.",
            if apache_active { "apache2" } else { "nginx" }
        );
    } else {
        println!("No hay servidor web activo. Instalando nginx...");
        let pkg_name = match pm {
            PackageManager::Apt => "nginx",
            PackageManager::Dnf => "nginx",
        };
        install_packages(pm, sudo_cmd, &[pkg_name])?;

        let status = SyncCommand::new(sudo_cmd)
            .args(["systemctl", "enable", "--now", "nginx"])
            .status()
            .context("no pude habilitar nginx")?;
        if !status.success() {
            bail!("systemctl enable --now nginx falló");
        }
    }

    // 2. Asegurar que tu usuario pueda escribir en /var/www/html sin sudo
    //    (evita el problema de sudo sin TTY dentro de publish.sh)
    let user = std::env::var("USER").context("no pude leer $USER")?;
    let status = SyncCommand::new(sudo_cmd)
        .args(["chown", "-R", &format!("{user}:{user}"), "/var/www/html"])
        .status()
        .context("no pude ajustar permisos de /var/www/html")?;
    if !status.success() {
        bail!("chown sobre /var/www/html falló");
    }

    Ok(())
}
