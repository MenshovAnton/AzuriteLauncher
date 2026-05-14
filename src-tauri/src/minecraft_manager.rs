use anyhow::{anyhow, Result};
use futures::{stream, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::Semaphore,
    time::{sleep, Duration},
};
use crate::path_manager::Paths;

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<Manifest>,
}

#[derive(Deserialize)]
struct Manifest {
    id: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub assets: Option<String>,
    pub asset_index: Option<AssetIndexInfo>,
    downloads: Downloads,
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
    pub id: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Downloads {
    client: Option<Download>,
}

#[derive(Deserialize)]
struct Download {
    sha1: String,
    url: String,
}

#[derive(Clone)]
struct DownloadTask {
    url: String,
    path: PathBuf,
    sha1: Option<String>,
}

#[derive(Deserialize)]
struct AssetIndexJson {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
}

#[derive(Deserialize)]
struct Library {
    downloads: Option<LibraryDownloads>,
    rules: Option<Vec<Rule>>,
}

#[derive(Deserialize)]
struct LibraryDownloads {
    artifact: Option<Artifact>,
    classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Deserialize)]
struct Artifact {
    path: String,
    sha1: String,
    url: String,
}

#[derive(Deserialize)]
struct Rule {
    action: String,
    os: Option<OsRule>,
}

#[derive(Deserialize)]
struct OsRule {
    name: Option<String>,
}

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

const VERSION_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

const ASSET_BASE: &str =
    "https://resources.download.minecraft.net";

const MAX_RETRIES: usize = 5;
const PARALLEL_DOWNLOADS: usize = 16;

pub struct MinecraftDownloader {
    paths: Paths,
    client: Client,
}

impl MinecraftDownloader {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths: paths.into(),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .user_agent("CubeXLauncher/1.0")
                .tcp_keepalive(Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    pub async fn install_version(&self, version_id: &str) -> Result<()> {
        println!("Loading manifest...");

        let manifest = self.load_manifest().await?;

        let version = manifest
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .ok_or_else(|| anyhow!("Minecraft version not found!"))?;

        println!("Loading version json...");

        let response_text = self
            .client
            .get(&version.url)
            .send()
            .await?
            .text()
            .await?;

        let version_dir = self
            .paths
            .versions
            .join(version_id);

        fs::create_dir_all(&version_dir).await?;

        let version_json_path =
            version_dir.join(format!("{version_id}.json"));

        fs::write(&version_json_path, &response_text).await?;

        let version_json: VersionJson =
            serde_json::from_str(&response_text)?;

        let mut tasks = Vec::<DownloadTask>::new();

        if let Some(client) = &version_json.downloads.client {
            tasks.push(DownloadTask {
                url: client.url.clone(),
                path: version_dir.join(format!(
                    "{}.jar",
                    version_json.id
                )),
                sha1: Some(client.sha1.clone()),
            });
        }

        for lib in &version_json.libraries {
            if !self.library_allowed(lib) {
                continue;
            }

            if let Some(downloads) = &lib.downloads {
                if let Some(artifact) = &downloads.artifact {
                    tasks.push(download_task(artifact, self.paths.clone()));
                }

                if let Some(classifiers) = &downloads.classifiers {
                    let native_key = if cfg!(target_os = "windows") {
                        "natives-windows"
                    } else if cfg!(target_os = "linux") {
                        "natives-linux"
                    } else {
                        "natives-osx"
                    };

                    if let Some(native) =
                        classifiers.get(native_key)
                    {
                        tasks.push(download_task(native, self.paths.clone()));
                    }
                }
            }
        }

        println!("Loading assets...");

        let asset_index = version_json
            .asset_index
            .as_ref()
            .ok_or_else(|| anyhow!("Missing asset index"))?;

        let asset_index_text = self
            .client
            .get(&asset_index.url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let indexes_dir =
            self.paths.assets.join("indexes");

        fs::create_dir_all(&indexes_dir).await?;

        let asset_index_path = indexes_dir.join(
            format!("{}.json", asset_index.id),
        );

        fs::write(
            &asset_index_path,
            &asset_index_text,
        )
            .await?;
        
        let asset_index_json: AssetIndexJson =
            serde_json::from_str(&asset_index_text)?;
        
        let assets_id = version_json
            .assets
            .as_deref()
            .unwrap_or("");

        let is_pre_1_6 =
            assets_id == "pre-1.6";

        let is_virtual =
            assets_id == "legacy";
        
        for (resource_path, asset) in asset_index_json.objects {
            let hash = asset.hash;

            let url = format!(
                "{}/{}/{}",
                ASSET_BASE,
                &hash[..2],
                hash
            );

            let path = if is_pre_1_6 {
                self.paths
                    .instances
                    .join(version_id)
                    .join("minecraft")
                    .join("resources")
                    .join(&resource_path)

            } else if is_virtual {
                self.paths
                    .assets
                    .join("virtual")
                    .join("legacy")
                    .join(&resource_path)

            } else {
                self.paths
                    .assets
                    .join("objects")
                    .join(&hash[..2])
                    .join(&hash)
            };

            tasks.push(DownloadTask {
                url,
                path,
                sha1: Some(hash),
            });
        }
        let mut filtered = Vec::new();

        for task in tasks {
            if needs_download(&task).await {
                filtered.push(task);
            }
        }

        println!(
            "Downloading {} files...",
            filtered.len()
        );

        self.download_tasks(filtered).await?;

        println!("Done!");

        Ok(())
    }

    async fn download_tasks(&self, tasks: Vec<DownloadTask>) -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(PARALLEL_DOWNLOADS));

        let results = stream::iter(tasks)
            .map(|task| {
                let semaphore = semaphore.clone();
                let this = self;

                async move {
                    let _permit = semaphore.acquire().await.unwrap();

                    match this.process_task(task).await {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e),
                    }
                }
            })
            .buffer_unordered(PARALLEL_DOWNLOADS)
            .collect::<Vec<_>>()
            .await;

        let failed = results
            .into_iter()
            .filter_map(|r| r.err())
            .collect::<Vec<_>>();

        if !failed.is_empty() {
            return Err(anyhow!(
            "{} files failed to download!",
            failed.len()
        ));
        }

        Ok(())
    }

    async fn process_task(&self, task: DownloadTask) -> Result<()> {
        if task.path.exists() {
            if let Some(hash) = &task.sha1 {
                if verify_sha1(&task.path, hash).await? {
                    return Ok(());
                }
            }
        }
        
        for attempt in 1..=MAX_RETRIES {
            match self.download_file(&task).await {
                Ok(_) => {
                    if let Some(hash) = &task.sha1 {
                        if !verify_sha1(&task.path, hash).await? {
                            return Err(anyhow!("SHA1 mismatch"));
                        }
                    }

                    return Ok(());
                }

                Err(e) => {
                    if attempt >= MAX_RETRIES {
                        return Err(e);
                    }

                    sleep(Duration::from_millis(
                        500 * attempt as u64,
                    ))
                        .await;
                }
            }
        }

        Err(anyhow!("Failed to download!"))
    }

    async fn download_file(&self, task: &DownloadTask) -> Result<()> {
        if let Some(parent) = task.path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let response = self
            .client
            .get(&task.url)
            .send()
            .await?
            .error_for_status()?;

        let bytes = response.bytes().await?;

        let mut file = fs::File::create(&task.path).await?;
        file.write_all(&bytes).await?;
        file.flush().await?;

        drop(file);

        Ok(())
    }

    async fn load_manifest(&self) -> Result<VersionManifest> {
        Ok(self
            .client
            .get(VERSION_MANIFEST)
            .send()
            .await?
            .json()
            .await?)
    }

    fn library_allowed(&self, lib: &Library) -> bool {
        let Some(rules) = &lib.rules else {
            return true;
        };

        let mut allowed = false;

        for rule in rules {
            let os_name = rule
                .os
                .as_ref()
                .and_then(|o| o.name.as_deref());

            let current_os = if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else {
                "osx"
            };

            let matches = match os_name {
                Some(name) => name == current_os,
                None => true,
            };

            match rule.action.as_str() {
                "allow" if matches => allowed = true,
                "disallow" if matches => return false,
                _ => {}
            }
        }

        allowed
    }
}

async fn verify_sha1(path: &Path, expected: &str) -> Result<bool> {
    let data = fs::read(path).await?;

    let mut hasher = Sha1::new();
    hasher.update(&data);

    let result = hex::encode(hasher.finalize());

    Ok(result == expected)
}

async fn needs_download(task: &DownloadTask) -> bool {
    if fs::metadata(&task.path).await.is_err() {
        return true;
    }

    if let Some(hash) = &task.sha1 {
        match verify_sha1(&task.path, hash).await {
            Ok(valid) => !valid,
            Err(_) => true,
        }
    } else {
        false
    }
}

fn download_task(artifact: &Artifact, paths: Paths) -> DownloadTask {
    DownloadTask {
        url: artifact.url.clone(),
        path: paths
            .libraries
            .join(&artifact.path),
        sha1: Some(artifact.sha1.clone()),
    }
}