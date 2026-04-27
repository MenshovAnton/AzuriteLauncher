// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launch_minecraft;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    cubexlauncher_lib::run()
}

#[tauri::command]
fn start(dir: String, ver: String) {
    let cfg = launch_minecraft::LaunchConfig{java_path: "C:/Users/Waysoon/AppData/Roaming/PrismLauncher/java/java-runtime-epsilon/bin/javaw.exe".to_string(),
        game_dir: dir,
        version: ver,
        username: "Test".to_string()};

    launch_minecraft::launch(cfg).expect("failed to launch!");
}
