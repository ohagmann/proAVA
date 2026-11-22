use std::path::PathBuf;

/// `~/.config/proava` on Linux, `%APPDATA%\proava` on Windows (roaming
/// profile: the settings follow the user in an office network),
/// `~/Library/Application Support/proava` on macOS.
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(dirs) = directories::BaseDirs::new() {
        return dirs.config_dir().join("proava");
    }
    directories::ProjectDirs::from_path(PathBuf::from("proava"))
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".proava"))
}

/// `~/.cache/proava` on Linux, `%LOCALAPPDATA%\proava\cache` on
/// Windows: copies that may be thrown away (catalogue lists).
pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from_path(PathBuf::from("proava"))
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".proava-cache"))
}

/// The personal folder (`%USERPROFILE%` on Windows).
pub fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The user's Downloads folder (XDG `XDG_DOWNLOAD_DIR`, Windows Known
/// Folder), else `~/Downloads`.
pub fn download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(PathBuf::from))
        .unwrap_or_else(|| home_dir().join("Downloads"))
}
