#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launch_minecraft;
mod minecraft_manager;
mod path_manager;

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start, minecraft_manager::get_versions])
        .run(tauri::generate_context!())
        .expect("Error while running tauri application.");

    azuritelauncher_lib::run()
}

#[tauri::command]
async fn start(jvm_path: String, game_version: String, username: String) {
    let launch_config = launch_minecraft::LaunchConfig
    {
        java_path: jvm_path,
        root: path_manager::get_app_directory(),
        version: game_version,
        username
    };

    println!("Start launching Minecraft {}.", launch_config.version);
    launch_minecraft::launch(launch_config).await.expect("Launch Minecraft failed.");
}