#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use directories::BaseDirs;

mod launch_minecraft;
mod minecraft_download;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start, minecraft_download::get_versions])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    cubexlauncher_lib::run()
}

#[tauri::command]
async fn start(jvm_path: String, game_version: String, username: String) {
    let launch_config = launch_minecraft::LaunchConfig
    {
        java_path: jvm_path,
        game_dir: app_directory(),
        version: game_version,
        username
    };

    println!("launching minecraft {}", launch_config.version);
    launch_minecraft::launch(launch_config).await.expect("launch failed");
}

fn app_directory() -> String {
    let base = BaseDirs::new().unwrap();
    let dir = base.data_dir().join("CubeXLauncher");

    if !dir.exists() {
        std::fs::create_dir_all(&dir).unwrap();
    }

    dir.to_str().unwrap().to_string()
}