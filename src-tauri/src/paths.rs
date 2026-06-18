/// Resolve the OS-specific app data directory for persistent storage.
pub fn get_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .map(|p| p.join("OpenCodeUsagePanel"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|p| p.join(".local/share/opencode-usage-panel"))
    }
}
