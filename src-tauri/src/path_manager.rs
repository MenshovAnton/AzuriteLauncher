use std::path::PathBuf;

#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub versions: PathBuf,
    pub assets: PathBuf,
    pub libraries: PathBuf,
    pub native_libraries: PathBuf,
    pub instances: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            versions: root.join("versions"),
            assets: root.join("assets"),
            libraries: root.join("libraries"),
            native_libraries: root.join("cache").join("launch").join("natives"),
            instances: root.join("instances"),
            root,
        }
    }
}

pub fn get_app_directory() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("CubeXLauncher")
    }

    #[cfg(target_os = "linux")]
    {
        PathBuf::from(std::env::var("HOME").unwrap())
            .join("CubeXLauncher")
    }

    #[cfg(target_os = "macos")]
    {
        PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library")
            .join("Application Support")
            .join("CubeXLauncher")
    }
}