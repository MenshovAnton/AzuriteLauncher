use std::{fs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs::File;
use std::io::Write;
use anyhow::Result;
use serde::Deserialize;
use zip::ZipArchive;
use rayon::prelude::*;
use std::io;

use crate::minecraft_download;

#[derive(Debug, Deserialize)]
struct VersionJson {
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
struct Library {
    downloads: Option<Downloads>,
    rules: Option<Vec<Rule>>,
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    os: Option<OsRule>,
}

#[derive(Debug, Deserialize)]
struct OsRule {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    artifact: Option<Artifact>,
    classifiers: Option<std::collections::HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize)]
struct Artifact {
    path: String,
}


pub struct LaunchConfig {
    pub java_path: String,
    pub game_dir: String,
    pub version: String,
    pub username: String,
}

pub async fn launch(cfg: LaunchConfig) -> Result<()> {
    validate_client(&cfg.version, &cfg.game_dir).await;

    let version_dir = format!("{}/versions/{}", cfg.game_dir, cfg.version);
    let client_jar = format!("{}/{}.jar", version_dir, cfg.version);
    let libs_dir = format!("{}/libraries", cfg.game_dir);
    let natives_dir = format!("{}/cache/launch/natives", cfg.game_dir);

    extract_natives(Path::new(&libs_dir), Path::new(&natives_dir), Path::new(&format!("{}/versions/{}/{}.json", cfg.game_dir, cfg.version, cfg.version))).expect("couldn't extract natives");

    let data = fs::read_to_string(&format!("{}/versions/{}/{}.json", cfg.game_dir, cfg.version, cfg.version))?;
    let version: VersionJson = serde_json::from_str(&data)?;

    let classpath = build_classpath(&libs_dir, &client_jar, &version)?;

    let uuid = generate_offline_uuid(&cfg.username);

    let args_path = format!("{}/launch_args.txt", cfg.game_dir);

    let content = build_args_file(&classpath, &natives_dir, &cfg, &uuid);
    write_args_file(&args_path, content)?;

    Command::new(&cfg.java_path)
        .arg(format!("@{}", args_path))
        .spawn()?
        .wait()?;

    Ok(())
}

fn build_classpath(libs_dir: &str, client_jar: &str, version: &VersionJson) -> std::io::Result<String> {
    let mut jars = Vec::new();

    for lib in &version.libraries {
        if !lib_is_allowed(&lib.rules) {
            continue;
        }

        if let Some(downloads) = &lib.downloads {
            if let Some(artifact) = &downloads.artifact {
                let full_path = format!("{}/{}", libs_dir, artifact.path);
                if let Ok(p) = canonical(&full_path) {
                    jars.push(p);
                } else {
                    println!("Missing file: {}", full_path);
                }
            }
        }
    }

    jars.push(canonical(client_jar)?);

    let sep = if cfg!(windows) { ";" } else { ":" };
    Ok(jars.join(sep))
}

fn canonical(path: &str) -> std::io::Result<String> {
    Ok(PathBuf::from(path)
        .canonicalize()?
        .to_string_lossy()
        .to_string())
}

// NOTE: This is a temporary measure for testing functionality.
fn generate_offline_uuid(username: &str) -> String {
    use md5;

    let input = format!("OfflinePlayer:{}", username);
    let digest = md5::compute(input);

    format!(
        "{:02x?}{:02x?}{:02x?}{:02x?}-\
         {:02x?}{:02x?}-\
         {:02x?}{:02x?}-\
         {:02x?}{:02x?}-\
         {:02x?}{:02x?}{:02x?}{:02x?}{:02x?}{:02x?}",
        digest[0], digest[1], digest[2], digest[3],
        digest[4], digest[5],
        digest[6], digest[7],
        digest[8], digest[9],
        digest[10], digest[11], digest[12], digest[13], digest[14], digest[15]
    ).replace(['[', ']', ',', ' '], "")
}

fn write_args_file(path: &str, content: String) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn build_args_file(
    classpath: &str,
    natives: &str,
    cfg: &LaunchConfig,
    uuid: &str) -> String {
    format!(
        r#"-Xmx2G
        -Xms1G
        -Djava.library.path={}
        -cp
        {}
        net.minecraft.client.main.Main
        --username {}
        --uuid {}
        --accessToken offline
        --version {}
        --gameDir {}/instance/{}/minecraft/
        --assetsDir {}/assets
        --assetIndex {}
        --userProperties {}
        "#,
        natives,
        classpath,
        cfg.username,
        uuid,
        cfg.version,
        cfg.game_dir,
        cfg.version,
        cfg.game_dir,
        cfg.version,
        "{}"
    )
}

fn lib_is_allowed(rules: &Option<Vec<Rule>>) -> bool {
    let Some(rules) = rules else {
        return true; // нет правил = разрешено
    };

    let mut allowed = false;

    for rule in rules {
        match rule.action.as_str() {
            "allow" => {
                if let Some(os) = &rule.os {
                    if let Some(name) = &os.name {
                        if name == get_current_os() {
                            allowed = true;
                        }
                    } else {
                        allowed = true;
                    }
                } else {
                    allowed = true;
                }
            }

            "disallow" => {
                if let Some(os) = &rule.os {
                    if let Some(name) = &os.name {
                        if name == get_current_os() {
                            return false;
                        }
                    }
                }
            }

            _ => {}
        }
    }

    allowed
}

fn get_current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "unknown"
    }
}

async fn validate_client(version: &String, dir: &String) {
    println!("validating client");

    let base = Path::new(dir);

    let version_dir = base.join("versions").join(version);

    let json = version_dir.join(format!("{}.json", version));
    let jar  = version_dir.join(format!("{}.jar", version));

    let version_json = minecraft_download::download_version_json(version, dir).await.unwrap();

    if !file_ok(&json) || !file_ok(&jar) {
        println!("client not found");
        minecraft_download::download_client(version, dir, &version_json).await.expect("couldn't download minecraft client");
        minecraft_download::download_libraries(version, dir, &version_json).await.expect("couldn't download libraries");
    }

    let asset_index_file = base
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", version));

    if !file_ok(&asset_index_file) {
        println!("assets not found");
        minecraft_download::download_assets(version, dir, &version_json).await.expect("couldn't download launch minecraft assets");
    }
}

fn file_ok(path: &Path) -> bool {
    path.exists() && path.is_file() && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

pub fn extract_natives(libs_dir: &Path, natives_dir: &Path, version_json_path: &Path, ) -> io::Result<()> {
    if !natives_dir.exists() {
        fs::create_dir_all(natives_dir)?;
    } else {
        fs::remove_dir_all(natives_dir)?;
    }

    let json_str = fs::read_to_string(version_json_path)?;
    let version: VersionJson = serde_json::from_str(&json_str)?;

    let current_os = get_os();

    let native_jars: Vec<PathBuf> = version
        .libraries
        .into_iter()
        .filter(|lib| is_allowed(lib, current_os))
        .filter_map(|lib| {
            let classifiers = lib.downloads?.classifiers?;

            let key = format!("natives-{}", current_os);

            classifiers.get(&key).map(|artifact| {
                libs_dir.join(&artifact.path)
            })
        })
        .collect();

    native_jars.par_iter().for_each(|jar| {
        if let Err(e) = extract_jar(jar, natives_dir) {
            eprintln!("error {:?}: {}", jar, e);
        }
    });

    Ok(())
}

fn extract_jar(jar_path: &Path, out_dir: &Path) -> io::Result<()> {
    let file = File::open(jar_path)?;
    let mut zip = ZipArchive::new(file)?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name();

        if name.starts_with("META-INF") {
            continue;
        }

        if name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib") {
            let out_path = out_dir.join(
                Path::new(name).file_name().unwrap()
            );

            let mut out = File::create(out_path)?;
            io::copy(&mut file, &mut out)?;
        }
    }

    Ok(())
}

fn get_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "osx",
        _ => "windows",
    }
}

fn is_allowed(lib: &Library, current_os: &str) -> bool {
    if lib.rules.is_none() {
        return true;
    }

    let mut allowed = false;

    for rule in lib.rules.as_ref().unwrap() {
        let applies = match &rule.os {
            Some(os) => os.name.as_deref() == Some(current_os),
            None => true,
        };

        if applies {
            allowed = rule.action == "allow";
        }
    }

    allowed
}