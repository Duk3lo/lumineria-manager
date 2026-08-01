use std::path::{Component, Path, PathBuf};

pub(crate) fn read_env_value(env_data: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    env_data.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|v| v.trim().trim_matches('"').to_string())
    })
}

pub(crate) fn base_dir_for_scope(root: &Path, id: &str, scope: protocol::FileScope) -> PathBuf {
    match scope {
        protocol::FileScope::Packwiz => root.join(id).join("packwiz"),
        protocol::FileScope::ServerRoot => root.join(id),
    }
}

pub(crate) fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut out = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("ruta inválida o fuera del modpack: '{rel}'"));
            }
        }
    }
    Ok(out)
}