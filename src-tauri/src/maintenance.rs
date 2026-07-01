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
const BACKUPS_DIR: &str = "backups";
const MAX_AUTO_BACKUPS: usize = 7;

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

pub(crate) fn auto_backup(
    settings: AppSettings,
    history: Vec<HistoryEntry>,
    cache: AppDataSnapshot,
) -> Result<Option<String>, String> {
    auto_backup_at(&data_dir(), &settings, &history, &cache, Utc::now())
}

pub(crate) fn should_auto_backup() -> bool {
    let dir = data_dir().join(BACKUPS_DIR);
    let today = Utc::now().format("%Y%m%d").to_string();
    !backup_exists_today(&dir, &today)
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

    let mut backup_bytes = 0u64;
    let mut backup_count = 0u32;
    let backup_dir = data_dir.join(BACKUPS_DIR);

    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if !metadata.is_file() {
                    continue;
                }
                backup_bytes += metadata.len();
                backup_count += 1;
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
        backup_bytes,
        backup_count,
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

fn auto_backup_at(
    data_dir: &Path,
    settings: &AppSettings,
    history: &[HistoryEntry],
    cache: &AppDataSnapshot,
    now: DateTime<Utc>,
) -> Result<Option<String>, String> {
    let backup_dir = ensure_backups_dir(data_dir)?;
    let today = now.format("%Y%m%d").to_string();

    // Skip if today's backup already exists
    if backup_exists_today(&backup_dir, &today) {
        return Ok(None);
    }

    let path = backup_dir.join(format!("auto-backup-{}.json", today));

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

    // Rotate old backups, keeping only the most recent MAX_AUTO_BACKUPS
    rotate_backups(&backup_dir)?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

fn ensure_backups_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let backup_dir = data_dir.join(BACKUPS_DIR);
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    Ok(backup_dir)
}

fn backup_exists_today(backup_dir: &Path, today: &str) -> bool {
    let filename = format!("auto-backup-{}.json", today);
    backup_dir.join(filename).exists()
}

fn rotate_backups(backup_dir: &Path) -> Result<(), String> {
    let mut backups: Vec<_> = std::fs::read_dir(backup_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("auto-backup-"))
        .collect();

    if backups.len() <= MAX_AUTO_BACKUPS {
        return Ok(());
    }

    // Sort by filename descending (newest first, since date is in name)
    backups.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

    // Remove oldest backups beyond the limit
    for old in backups.iter().skip(MAX_AUTO_BACKUPS) {
        if let Err(e) = std::fs::remove_file(old.path()) {
            eprintln!(
                "[Backup] Failed to remove old backup {:?}: {}",
                old.path(),
                e
            );
        }
    }

    Ok(())
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

    #[test]
    fn auto_backup_creates_file_in_backups_dir() {
        let dir = unique_temp_path("auto-backup-create");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let settings = AppSettings::default();
        let cache = AppDataSnapshot::empty();
        let now = Utc::now();

        let result = auto_backup_at(&dir, &settings, &[], &cache, now);

        assert!(result.is_ok());
        let path_opt = result.unwrap();
        assert!(path_opt.is_some());
        let path = path_opt.unwrap();
        assert!(path.contains("auto-backup-"));
        assert!(std::path::Path::new(&path).exists());

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn auto_backup_skips_when_today_already_exists() {
        let dir = unique_temp_path("auto-backup-skip");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let settings = AppSettings::default();
        let cache = AppDataSnapshot::empty();
        let now = Utc::now();

        // First backup should succeed
        let first = auto_backup_at(&dir, &settings, &[], &cache, now);
        assert!(first.is_ok());
        assert!(first.unwrap().is_some());

        // Second backup same day should return None
        let second = auto_backup_at(&dir, &settings, &[], &cache, now);
        assert!(second.is_ok());
        assert!(second.unwrap().is_none());

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn rotate_backups_keeps_only_max_files() {
        let dir = unique_temp_path("auto-backup-rotate");
        let backup_dir = dir.join(BACKUPS_DIR);
        std::fs::create_dir_all(&backup_dir).expect("create backups dir");

        // Create MAX_AUTO_BACKUPS + 3 files (simulating 10 backups when max is 7)
        for i in 0..(MAX_AUTO_BACKUPS + 3) {
            let filename = format!("auto-backup-202601{:02}.json", i + 1);
            std::fs::write(backup_dir.join(&filename), b"{}").expect("write backup file");
        }

        let count_before = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .ok()
                    .map(|e| e.file_name().to_string_lossy().starts_with("auto-backup-"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count_before, MAX_AUTO_BACKUPS + 3);

        rotate_backups(&backup_dir).expect("rotate should succeed");

        let count_after = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .ok()
                    .map(|e| e.file_name().to_string_lossy().starts_with("auto-backup-"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count_after, MAX_AUTO_BACKUPS);

        // The newest files (highest dates) should remain
        assert!(backup_dir.join("auto-backup-20260110.json").exists());
        assert!(backup_dir.join("auto-backup-20260109.json").exists());
        // The oldest files should be gone
        assert!(!backup_dir.join("auto-backup-20260101.json").exists());
        assert!(!backup_dir.join("auto-backup-20260102.json").exists());
        assert!(!backup_dir.join("auto-backup-20260103.json").exists());

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn backup_exists_today_returns_false_when_no_file() {
        let dir = unique_temp_path("backup-exists-empty");
        std::fs::create_dir_all(&dir).expect("create temp dir");

        assert!(!backup_exists_today(&dir, "20260619"));

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn backup_exists_today_returns_true_when_file_present() {
        let dir = unique_temp_path("backup-exists-yes");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("auto-backup-20260619.json"), b"{}").expect("write");

        assert!(backup_exists_today(&dir, "20260619"));
        assert!(!backup_exists_today(&dir, "20260620"));

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }

    #[test]
    fn local_data_status_counts_backups() {
        let dir = unique_temp_path("status-backups");
        let backups = dir.join(BACKUPS_DIR);
        std::fs::create_dir_all(&backups).expect("create backups directory");
        std::fs::write(backups.join("auto-backup-20260618.json"), b"12345").expect("write backup");
        std::fs::write(backups.join("auto-backup-20260619.json"), b"1234567890")
            .expect("write backup");

        let status = local_data_status_in(&dir);

        assert_eq!(status.backup_bytes, 15);
        assert_eq!(status.backup_count, 2);

        std::fs::remove_dir_all(&dir).expect("remove temp test directory");
    }
}
