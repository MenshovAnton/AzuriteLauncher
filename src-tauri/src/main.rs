// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launch_minecraft;
mod minecraft_versions_control;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start, minecraft_versions_control::get_versions])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    cubexlauncher_lib::run()
}

#[tauri::command]
async fn start(jvm_path: String, game_directory: String, game_version: String, username: String) {
    let launch_config = launch_minecraft::LaunchConfig
    {
        java_path: jvm_path,
        game_dir: game_directory,
        version: game_version,
        username
    };

    tokio::task::spawn_blocking(move || {launch_minecraft::launch(launch_config).expect("launch failed");});
}
