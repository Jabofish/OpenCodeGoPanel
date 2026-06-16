/// Resolve the OS-specific app data directory for persistent storage.
pub fn get_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join("OpenCodeUsagePanel"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join(".local/share/opencode-usage-panel"))
    }
}
