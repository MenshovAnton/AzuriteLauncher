use reqwest;

#[tauri::command]
pub async fn get_versions() -> Result<Vec<String>, String> {
    let res = reqwest::get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    let versions = res["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();

    Ok(versions)
}