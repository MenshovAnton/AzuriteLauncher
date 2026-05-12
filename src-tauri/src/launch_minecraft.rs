use std::{
    fs,
    fs::File,
    path::{Path, PathBuf},
    process::Command,
    io
};
use anyhow::Result;
use serde::Deserialize;
use zip::ZipArchive;
use rayon::prelude::*;
use serde_json::Value;

use crate::minecraft_manager;
use crate::path_manager::Paths;

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
    classifiers: Option<std::collections::HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize)]
struct Artifact {
    path: String,
}

pub struct LaunchConfig {
    pub java_path: String,
    pub root: PathBuf,
    pub version: String,
    pub username: String,
}

pub async fn launch(cfg: LaunchConfig) -> Result<()> {
    let paths = Paths::new(cfg.root);

    let downloader = minecraft_manager::MinecraftDownloader::new(paths.clone());
    downloader.install_version(cfg.version.as_str()).await?;

    let version_json_path = paths.versions.join(&cfg.version).join(format!("{}.json", cfg.version));

    extract_natives(&paths.libraries, &paths.native_libraries, &version_json_path)
        .expect("Couldn't extract natives!");

    let data = fs::read_to_string(version_json_path)?;
    let version_json: Value = serde_json::from_str(&data)?;

    let uuid = generate_offline_uuid(&cfg.username);

    let launch_args = build_launch_args(&version_json, paths, &cfg.version, &cfg.username, &uuid, "0");
    println!("JVM args:\n{}", launch_args.join("\n"));

    Command::new(&cfg.java_path)
        .args(&launch_args)
        .spawn()?
        .wait()?;

    Ok(())
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

pub fn build_launch_args(
    version_json: &Value,
    paths: Paths,
    version_name: &str,
    auth_player_name: &str,
    auth_uuid: &str,
    auth_access_token: &str,
) -> Vec<String> {
    let mut args = Vec::new();

    let mut classpath = Vec::new();

    if let Some(libs) = version_json["libraries"].as_array() {
        for lib in libs {
            if !check_rules(lib) {
                continue;
            }

            if lib.get("natives").is_some() {
                continue;
            }

            if let Some(artifact) = lib["downloads"].get("artifact") {
                if let Some(path) = artifact["path"].as_str() {
                    classpath.push(paths.libraries.join(PathBuf::from(path)).to_string_lossy().into_owned());
                }
            }
        }
    }

    let client_jar = paths.versions.join(&version_name).join(format!("{}.jar", version_name));
    classpath.push(client_jar.to_string_lossy().into_owned());

    if let Some(jvm_args) = version_json["arguments"]["jvm"].as_array() {
        for arg in jvm_args {
            if let Some(val) = parse_arg(arg) {
                args.push(replace_vars(val,
                                       paths.assets.to_str().unwrap(),
                                       paths.libraries.to_str().unwrap(),
                                       paths.native_libraries.to_str().unwrap(),
                                       version_name,
                                       &classpath.join(get_classpath_separator())));
            }
        }
    } else {
        args.push(format!("-Djava.library.path={}", paths.native_libraries.to_str().unwrap()));
        args.push(String::from("-cp"));
        args.push(format!("{}", &classpath.join(get_classpath_separator())));
    }

    let main_class = version_json["mainClass"]
        .as_str()
        .unwrap_or("net.minecraft.client.main.Main");

    args.push(main_class.to_string());

    let assets_index = version_json["assetIndex"]["id"].as_str().unwrap();
    println!("{}", assets_index);


    if let Some(legacy_args) = version_json["minecraftArguments"].as_str() {
        let parsed = parse_legacy_args(legacy_args);

        for arg in parsed {
            args.push(replace_legacy_vars(
                &arg,
                paths.instances.join(version_name).join("minecraft").to_str().unwrap(),
                paths.assets.to_str().unwrap(),
                assets_index,
                version_name,
                auth_player_name,
                auth_uuid,
                auth_access_token,
            ));
        }
    }

    if let Some(game_args) = version_json["arguments"]["game"].as_array() {
        for arg in game_args {
            if let Some(val) = parse_arg(arg) {
                args.push(replace_game_vars(
                    val,
                    paths.instances.join(version_name).join("minecraft").to_str().unwrap(),
                    paths.assets.to_str().unwrap(),
                    assets_index,
                    version_name,
                    auth_player_name,
                    auth_uuid,
                    auth_access_token,
                    "0",
                    "0"
                ));
            }
        }
    }

    args
}

fn parse_arg(arg: &Value) -> Option<&str> {
    match arg {
        Value::String(s) => Some(s),
        Value::Object(obj) => {
            if !check_rules(arg) {
                return None;
            }

            obj["value"].as_str()
        }
        _ => None,
    }
}

fn parse_legacy_args(arg_string: &str) -> Vec<String> {
    arg_string
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

fn check_rules(obj: &Value) -> bool {
    if let Some(rules) = obj["rules"].as_array() {
        for rule in rules {
            let action = rule["action"].as_str().unwrap_or("disallow");

            if let Some(os) = rule["os"]["name"].as_str() {
                let current = std::env::consts::OS;

                if os == "windows" && current != "windows" {
                    return action == "disallow";
                }
                if os == "linux" && current != "linux" {
                    return action == "disallow";
                }
                if os == "osx" && current != "macos" {
                    return action == "disallow";
                }
            }
        }
    }

    true
}

fn replace_vars(arg: &str,
                assets_dir: &str,
                libraries_dir: &str,
                natives_dir: &str,
                version: &str,
                classpath: &str)
                -> String {
    arg.replace("${natives_directory}", natives_dir)
        .replace("${launcher_name}", "CubeX Launcher")
        .replace("${launcher_version}", "1.0")
        .replace("${classpath_separator}", get_classpath_separator())
        .replace("${library_directory}", libraries_dir)
        .replace("${version_name}", version)
        .replace("${assets_root}", assets_dir)
        .replace("${classpath}", classpath)
}

fn replace_legacy_vars(arg: &str,
                       game_dir: &str,
                       assets_dir: &str,
                       assets_index: &str,
                       version: &str,
                       username: &str,
                       uuid: &str,
                       token: &str, ) -> String
{
    arg.replace("${auth_player_name}", username)
        .replace("${auth_uuid}", uuid)
        .replace("${auth_access_token}", token)
        .replace("${auth_session}", token)
        .replace("${version_name}", version)
        .replace("${game_directory}", game_dir)
        .replace("${assets_root}", assets_dir)
        .replace("${assets_index_name}", assets_index)
        .replace("${game_assets}", assets_dir)
        .replace("${user_type}", "legacy")
        .replace("${version_type}", "release")
        .replace("${user_properties}", "{}")
}

fn replace_game_vars(arg: &str,
                     game_dir: &str,
                     assets_dir: &str,
                     assets_index: &str,
                     version: &str,
                     username: &str,
                     uuid: &str,
                     token: &str,
                     client_id: &str,
                     xuid: &str, ) -> String
{
    arg.replace("${auth_player_name}", username)
        .replace("${auth_uuid}", uuid)
        .replace("${auth_access_token}", token)
        .replace("${version_name}", version)
        .replace("${clientid}", client_id)
        .replace("${auth_xuid}", xuid)
        .replace("${game_directory}", game_dir)
        .replace("${assets_root}", assets_dir)
        .replace("${assets_index_name}", assets_index)
        .replace("${user_type}", "msa")
        .replace("${version_type}", "release")
        .replace("--demo", "")
}

fn get_classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}
pub fn extract_natives(libs_dir: &Path,
                       natives_dir: &Path,
                       version_json_path: &Path, ) -> io::Result<()>
{
    if !natives_dir.exists() {
        fs::create_dir_all(natives_dir)?;
    } else {
        fs::remove_dir_all(natives_dir)?;
        fs::create_dir_all(natives_dir)?;
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