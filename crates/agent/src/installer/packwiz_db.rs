use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackEntry {
    pub title: String,
    pub mc_version: String,
    pub version_id: String,
    pub java_version: u32,
    pub loader_name: String,
    pub loader_url: String,
    pub packwiz_url: String,
    pub image: String,
}

pub async fn upsert_entry(
    database_dir: &Path,
    key: &str,
    entry: ModpackEntry,
) -> Result<()> {
    // Crear la carpeta lumineria_database si no existe
    fs::create_dir_all(database_dir).await?;
    let json_path = database_dir.join("modpacks.json");

    let mut map: HashMap<String, ModpackEntry> = if json_path.exists() {
        let data = fs::read_to_string(&json_path).await?;
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    };

    map.insert(key.to_string(), entry);

    let json = serde_json::to_string_pretty(&map)?;
    fs::write(json_path, json).await?;
    Ok(())
}

pub async fn remove_entry(database_dir: &Path, key: &str) -> Result<bool> {
    let json_path = database_dir.join("modpacks.json");
    if !json_path.exists() {
        return Ok(false);
    }
    let data = fs::read_to_string(&json_path).await?;
    let mut map: HashMap<String, ModpackEntry> = serde_json::from_str(&data).unwrap_or_default();
    let existed = map.remove(key).is_some();
    if existed {
        let json = serde_json::to_string_pretty(&map)?;
        fs::write(json_path, json).await?;
    }
    Ok(existed)
}