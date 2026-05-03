use reqwest;
use std::fs::{self, File};
use std::io::{Write};
use std::path::{Path};
use futures::stream::{self, StreamExt};
use tokio::io::AsyncWriteExt;

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

pub async fn download_version_json(version: &String, dir: &String) -> Result<serde_json::Value, String> {
    let manifest: serde_json::Value = reqwest::get(
        "https://launchermeta.mojang.com/mc/game/version_manifest.json"
    )
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let versions = manifest["versions"].as_array().unwrap();

    let version_info = versions
        .iter()
        .find(|v| v["id"] == *version)
        .ok_or("minecraft version not found")?;

    let version_url = version_info["url"]
        .as_str()
        .ok_or("no URL")?;

    let version_json: serde_json::Value = reqwest::get(version_url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let dir = format!("{}/versions/{}", dir, version);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let file_path = format!("{}/{}.json", dir, version);
    let mut file = File::create(&file_path).map_err(|e| e.to_string())?;
    let version_json_string = serde_json::to_string_pretty(&version_json).unwrap();
    file.write_all(version_json_string.as_bytes()).map_err(|e| e.to_string())?;

    Ok(version_json)
}

pub async fn download_client(version: &String, dir: &String, version_json: &serde_json::Value) -> Result<String, String> {
    println!("downloading client {}", version);

    let client_url = version_json["downloads"]["client"]["url"]
        .as_str()
        .ok_or("No client URL")?;

    let response = reqwest::get(client_url)
        .await
        .map_err(|e| e.to_string())?;

    let dir = format!("{}/versions/{}", dir, version);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let file_path = format!("{}/{}.jar", dir, version);
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }

    println!("successful downloaded client {}", version);
    Ok("successful downloaded client".to_string())
}

pub async fn download_assets(version: &String, dir: &String, version_json: &serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
    println!("downloading assets for {}", version);

    let asset_index_url = version_json["assetIndex"]["url"]
        .as_str()
        .ok_or("no asset index")?;

    let asset_index: serde_json::Value = reqwest::get(asset_index_url)
        .await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let objects = asset_index["objects"]
        .as_object()
        .ok_or("invalid assets")?;

    download_assets_parallel(objects.clone(), dir).await?;

    let index_path = format!("{}/assets/indexes/{}.json", dir, version);

    fs::create_dir_all(format!("{}/assets/indexes", dir))?;

    tokio::fs::write(
        &index_path,
        serde_json::to_vec(&asset_index).unwrap()
    ).await?;

    println!("successful downloaded assets {}", version);
    Ok("successful downloaded assets".into())
}

async fn download_assets_parallel(objects: serde_json::Map<String, serde_json::Value>, dir: &String) -> Result<(), String> {
    let base_path = &format!("{}/assets/objects", dir);
    fs::create_dir_all(base_path).map_err(|e| e.to_string())?;

    stream::iter(objects.into_iter())
        .map(|(_name, obj)| async move {
            let hash = obj["hash"].as_str().unwrap();

            let folder = &hash[0..2];
            let url = format!(
                "https://resources.download.minecraft.net/{}/{}",
                folder, hash
            );

            let dir = format!("{}/{}", base_path, folder);
            let file_path = format!("{}/{}", dir, hash);

            if Path::new(&file_path).exists() {
                return Ok(());
            }

            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

            let bytes = reqwest::get(&url)
                .await.map_err(|e| e.to_string())?
                .bytes()
                .await.map_err(|e| e.to_string())?;

            let mut file = File::create(&file_path).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;

            println!("downloading asset {}", file_path);

            Ok::<(), String>(())
        })
        .buffer_unordered(128)
        .collect::<Vec<_>>()
        .await;
    Ok(())
}

pub async fn download_libraries(version: &String, dir: &String, version_json: &serde_json::Value) -> Result<String, String> {
    println!("downloading libraries for {}", version);

    let libraries = version_json["libraries"]
        .as_array()
        .ok_or("No libraries")?
        .clone();

    let client = reqwest::Client::new();
    download_libraries_parallel(libraries, client, dir).await?;

    println!("successful downloaded libraries for {}", version);
    Ok("successful downloaded libraries".into())
}

async fn download_libraries_parallel(libraries: Vec<serde_json::Value>, client: reqwest::Client, dir: &String) -> Result<(), String> {
    stream::iter(libraries.into_iter())
        .map(|lib| {
            let client = client.clone();

            async move {
                if !lib_is_allowed(&lib) {
                    return Ok(());
                }

                if let Some(artifact) = lib["downloads"].get("artifact") {
                    let url = artifact["url"].as_str().unwrap();
                    let path = artifact["path"].as_str().unwrap();

                    let full_path = format!("{}/libraries/{}", dir, path);

                    write_libraries(&full_path, &client, url).await?;
                }

                if let Some(classifiers) = lib["downloads"].get("classifiers") {
                    let os_key = if cfg!(target_os = "windows") {
                        "natives-windows"
                    } else if cfg!(target_os = "linux") {
                        "natives-linux"
                    } else {
                        "natives-osx"
                    };

                    if let Some(native) = classifiers.get(os_key) {
                        let url = native["url"].as_str().unwrap();
                        let path = native["path"].as_str().unwrap();

                        let full_path = format!("{}/libraries/{}", dir, path);

                        write_libraries(&full_path, &client, url).await?;
                    }
                }

                Ok::<(), String>(())
            }
        })
        .buffer_unordered(24)
        .for_each(|res| async {
            if let Err(e) = res {
                println!("lib error: {}", e);
            }
        })
        .await;

    Ok(())
}

async fn write_libraries(full_path: &String, client: &reqwest::Client, url: &str) -> Result<(), String> {
    if !Path::new(full_path).exists() {
        println!("downloading lib {}", full_path);

        if let Some(parent) = Path::new(full_path).parent() {
            fs::create_dir_all(parent).unwrap();
        }

        let bytes = client
            .get(url)
            .send()
            .await.map_err(|e| e.to_string())?
            .bytes()
            .await.map_err(|e| e.to_string())?;

        fs::write(full_path, &bytes)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn lib_is_allowed(lib: &serde_json::Value) -> bool {
    if let Some(rules) = lib.get("rules") {
        let mut allowed = false;

        for rule in rules.as_array().unwrap() {
            let action = rule["action"].as_str().unwrap_or("disallow");

            let applies = if let Some(os) = rule.get("os") {
                let name = os["name"].as_str().unwrap_or("");

                (cfg!(target_os = "windows") && name == "windows") ||
                    (cfg!(target_os = "linux") && name == "linux") ||
                    (cfg!(target_os = "macos") && name == "osx")
            } else {
                true
            };

            if applies {
                allowed = action == "allow";
            }
        }

        return allowed;
    }

    true
}