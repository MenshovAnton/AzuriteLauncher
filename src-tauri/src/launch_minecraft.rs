use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs::File;
use std::io::Write;

pub struct LaunchConfig {
    pub java_path: String,
    pub game_dir: String,
    pub version: String,
    pub username: String,
}

pub fn launch(cfg: LaunchConfig) -> std::io::Result<()> {
    let version_dir = format!("{}/versions/{}", cfg.game_dir, cfg.version);
    let client_jar = format!("{}/{}.jar", version_dir, cfg.version);
    let libs_dir = format!("{}/libraries", cfg.game_dir);
    let natives_dir = format!("{}/natives", cfg.game_dir);

    let classpath = build_classpath(&libs_dir, &client_jar)?;

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

fn build_classpath(libs_dir: &str, client_jar: &str) -> std::io::Result<String> {
    let mut jars = Vec::new();

    collect_jars(Path::new(libs_dir), &mut jars)?;
    jars.push(canonical(client_jar)?);

    let sep = if cfg!(windows) { ";" } else { ":" };
    Ok(jars.join(sep))
}

fn collect_jars(dir: &Path, jars: &mut Vec<String>) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                collect_jars(&path, jars)?;
            } else if let Some(ext) = path.extension() {
                if ext == "jar" {
                    jars.push(canonical(path.to_str().unwrap())?);
                }
            }
        }
    }
    Ok(())
}

fn canonical(path: &str) -> std::io::Result<String> {
    Ok(PathBuf::from(path)
        .canonicalize()?
        .to_string_lossy()
        .to_string())
}

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
        --gameDir {}
        --assetsDir {}/assets
        --assetIndex {}
        "#,
        natives,
        classpath,
        cfg.username,
        uuid,
        cfg.version,
        cfg.game_dir,
        cfg.game_dir,
        cfg.version
    )
}