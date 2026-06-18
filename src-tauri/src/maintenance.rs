use crate::models::{AppDataSnapshot, DataFileHealth, HealthCheck, HistoryEntry, LocalDataStatus};
use crate::paths;
use crate::settings_store::AppSettings;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

const CACHE_FILE: &str = "opencode-cache.json";
const HISTORY_FILE: &str = "opencode-history.json";
const SETTINGS_FILE: &str = "opencode-settings.json";
const AUTH_FILE: &str = "opencode-auth.json";
const EXPORTS_DIR: &str = "exports";

pub(crate) enum ClearLocalDataEffect {
    None,
    ClearCache,
    ClearHistory,
}

pub(crate) fn local_data_status() -> LocalDataStatus {
    local_data_status_in(&data_dir())
}

pub(crate) fn backup_local_data(
    settings: AppSettings,
    history: Vec<HistoryEntry>,
    cache: AppDataSnapshot,
) -> Result<String, String> {
    backup_local_data_at(&data_dir(), &settings, &history, &cache, Utc::now())
}

pub(crate) fn clear_local_data(scope: &str) -> Result<ClearLocalDataEffect, String> {
    clear_local_data_in(&data_dir(), scope)
}

pub(crate) fn open_exports_folder() -> Result<String, String> {
    let export_dir = ensure_exports_dir(&data_dir())?;
    let path_str = export_dir.to_string_lossy().to_string();
    open_folder(&export_dir)?;
    Ok(path_str)
}

pub(crate) fn run_health_check(has_auth: bool, last_refresh_error: Option<String>) -> HealthCheck {
    health_check_in(&data_dir(), has_auth, last_refresh_error)
}

fn data_dir() -> PathBuf {
    paths::get_data_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn local_data_status_in(data_dir: &Path) -> LocalDataStatus {
    let mut export_bytes = 0u64;
    let mut export_count = 0u32;
    let export_dir = data_dir.join(EXPORTS_DIR);

    if let Ok(entries) = std::fs::read_dir(&export_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if !metadata.is_file() {
                    continue;
                }
                export_bytes += metadata.len();
                export_count += 1;
            }
        }
    }

    LocalDataStatus {
        data_dir: data_dir.to_string_lossy().into_owned(),
        cache_bytes: file_bytes(data_dir, CACHE_FILE),
        history_bytes: file_bytes(data_dir, HISTORY_FILE),
        settings_bytes: file_bytes(data_dir, SETTINGS_FILE),
        auth_bytes: file_bytes(data_dir, AUTH_FILE),
        export_bytes,
        export_count,
    }
}

fn backup_local_data_at(
    data_dir: &Path,
    settings: &AppSettings,
    history: &[HistoryEntry],
    cache: &AppDataSnapshot,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let export_dir = ensure_exports_dir(data_dir)?;
    let ts = now.format("%Y%m%d-%H%M%S");
    let path = export_dir.join(format!("opencode-backup-{}.json", ts));

    let backup = serde_json::json!({
        "version": 1,
        "createdAt": now.to_rfc3339(),
        "settings": settings,
        "history": history,
        "cache": cache,
        "auth": null,
    });

    std::fs::write(
        &path,
        serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

fn clear_local_data_in(data_dir: &Path, scope: &str) -> Result<ClearLocalDataEffect, String> {
    match scope {
        "cache" => Ok(ClearLocalDataEffect::ClearCache),
        "history" => {
            let _ = std::fs::remove_file(data_dir.join(HISTORY_FILE));
            Ok(ClearLocalDataEffect::ClearHistory)
        }
        "exports" => {
            clear_export_files(data_dir)?;
            Ok(ClearLocalDataEffect::None)
        }
        "settings" => {
            let _ = std::fs::remove_file(data_dir.join(SETTINGS_FILE));
            Ok(ClearLocalDataEffect::None)
        }
        _ => Err(format!(
            "Unknown scope: {}. Use cache, history, exports, or settings.",
            scope
        )),
    }
}

fn clear_export_files(data_dir: &Path) -> Result<(), String> {
    let export_dir = data_dir.join(EXPORTS_DIR);
    if !export_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&export_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
            std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn health_check_in(
    data_dir: &Path,
    has_auth: bool,
    last_refresh_error: Option<String>,
) -> HealthCheck {
    let (data_dir_exists, data_dir_available, data_dir_error) = local_data_dir_health(data_dir);
    let cache_file = local_data_file_health(&data_dir.join(CACHE_FILE));
    let settings_file = local_data_file_health(&data_dir.join(SETTINGS_FILE));
    let history_file = local_data_file_health(&data_dir.join(HISTORY_FILE));
    let auth_file = local_data_file_health(&data_dir.join(AUTH_FILE));
    let cache_ok = local_data_file_ok(&cache_file);
    let settings_ok = local_data_file_ok(&settings_file);
    let history_ok = local_data_file_ok(&history_file);

    HealthCheck {
        has_auth,
        cache_ok,
        settings_ok,
        history_ok,
        data_dir: data_dir.to_string_lossy().into_owned(),
        data_dir_exists,
        data_dir_available,
        data_dir_error,
        cache_file,
        settings_file,
        history_file,
        auth_file,
        last_refresh_error,
    }
}

fn ensure_exports_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let export_dir = data_dir.join(EXPORTS_DIR);
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
    Ok(export_dir)
}

fn file_bytes(data_dir: &Path, name: &str) -> u64 {
    std::fs::metadata(data_dir.join(name))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn local_data_file_health(path: &Path) -> DataFileHealth {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return DataFileHealth {
                    exists: true,
                    readable: false,
                    bytes: 0,
                    error: Some("Path exists but is not a file".to_string()),
                };
            }

            match std::fs::File::open(path) {
                Ok(_) => DataFileHealth {
                    exists: true,
                    readable: true,
                    bytes: metadata.len(),
                    error: None,
                },
                Err(error) => DataFileHealth {
                    exists: true,
                    readable: false,
                    bytes: metadata.len(),
                    error: Some(error.to_string()),
                },
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DataFileHealth {
            exists: false,
            readable: false,
            bytes: 0,
            error: None,
        },
        Err(error) => DataFileHealth {
            exists: false,
            readable: false,
            bytes: 0,
            error: Some(error.to_string()),
        },
    }
}

fn local_data_file_ok(status: &DataFileHealth) -> bool {
    !status.exists || (status.readable && status.error.is_none())
}

fn local_data_dir_health(data_dir: &Path) -> (bool, bool, Option<String>) {
    match std::fs::metadata(data_dir) {
        Ok(metadata) if metadata.is_dir() => (true, true, None),
        Ok(_) => (
            true,
            false,
            Some("Path exists but is not a directory".to_string()),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::create_dir_all(data_dir) {
                Ok(_) => (true, true, None),
                Err(error) => (false, false, Some(error.to_string())),
            }
        }
        Err(error) => (false, false, Some(error.to_string())),
    }
}

fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "opencode-panel-{}-{}-{}",
            name,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn local_data_status_counts_files_in_exports() {
        let dir = unique_temp_path("status");
        let exports = dir.join(EXPORTS_DIR);
        std::fs::create_dir_all(&exports).expect("create exports directory");
        std::fs::write(dir.join(CACHE_FILE), b"cache").expect("write cache file");
        std::fs::write(exports.join("a.json"), b"123").expect("write export file");
        std::fs::write(exports.join("b.csv"), b"12345").expect("write export file");
        std::fs::create_dir_all(exports.join("nested")).expect("create nested export directory");

        let status = local_data_status_in(&dir);

        assert_eq!(status.cache_bytes, 5);
        assert_eq!(status.export_bytes, 8);
        assert_eq!(status.export_count, 2);

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn clear_exports_removes_files_but_keeps_directories() {
        let dir = unique_temp_path("clear-exports");
        let exports = dir.join(EXPORTS_DIR);
        let nested = exports.join("nested");
        std::fs::create_dir_all(&nested).expect("create nested export directory");
        let file = exports.join("backup.json");
        std::fs::write(&file, b"{}").expect("write export file");

        let effect = clear_local_data_in(&dir, "exports").expect("clear exports");

        assert!(matches!(effect, ClearLocalDataEffect::None));
        assert!(!file.exists());
        assert!(nested.is_dir());

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn local_data_file_health_reports_missing_file_without_error() {
        let path = unique_temp_path("missing-file");

        let status = local_data_file_health(&path);

        assert!(!status.exists);
        assert!(!status.readable);
        assert_eq!(status.bytes, 0);
        assert!(status.error.is_none());
        assert!(local_data_file_ok(&status));
    }

    #[test]
    fn local_data_file_health_reports_readable_file_size() {
        let dir = unique_temp_path("readable-file");
        std::fs::create_dir_all(&dir).expect("create temp test directory");
        let path = dir.join(CACHE_FILE);
        std::fs::write(&path, br#"{"ok":true}"#).expect("write temp test file");

        let status = local_data_file_health(&path);

        assert!(status.exists);
        assert!(status.readable);
        assert_eq!(status.bytes, 11);
        assert!(status.error.is_none());
        assert!(local_data_file_ok(&status));

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn local_data_file_health_reports_directory_as_not_ok() {
        let dir = unique_temp_path("directory-file");
        std::fs::create_dir_all(&dir).expect("create temp test directory");

        let status = local_data_file_health(&dir);

        assert!(status.exists);
        assert!(!status.readable);
        assert_eq!(status.bytes, 0);
        assert!(status.error.is_some());
        assert!(!local_data_file_ok(&status));

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn local_data_dir_health_creates_missing_directory() {
        let dir = unique_temp_path("data-dir");

        let status = local_data_dir_health(&dir);

        assert_eq!(status, (true, true, None));
        assert!(dir.is_dir());

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }
}
