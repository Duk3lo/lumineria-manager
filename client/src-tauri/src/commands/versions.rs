#[tauri::command]
pub async fn fetch_paper_project_versions(project: String) -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let url = format!("https://fill.papermc.io/v3/projects/{}", project);
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "LumineriaManager/2.0 (contacto: admin@lumineria.local)",
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} al consultar {}",
            response.status(),
            project
        ));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let versions_obj = json["versions"]
        .as_object()
        .ok_or_else(|| "Formato inesperado: falta 'versions'".to_string())?;

    let versions: Vec<String> = versions_obj
        .values()
        .filter_map(|v| v.as_array())
        .flatten()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Ok(versions)
}

#[tauri::command]
pub async fn fetch_neoforge_versions() -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let versions = json["versions"]
        .as_array()
        .ok_or_else(|| "Formato inesperado".to_string())?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Ok(versions)
}

#[tauri::command]
pub async fn fetch_forge_versions() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json)
}
