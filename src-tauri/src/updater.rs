use crate::settings_store::SettingsStore;
use serde::Serialize;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Information about an available update, shared with the frontend.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Status events emitted to the frontend during the update lifecycle.
#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum UpdateStatus {
    Checking,
    Available { info: UpdateInfo },
    Downloading { progress: f64, total: Option<u64> },
    Downloaded,
    Installing,
    UpToDate,
    Error { message: String },
}

/// Managed state: holds the pending update between check → download → install.
pub struct PendingUpdate {
    pub info: Mutex<Option<UpdateInfo>>,
    package: Mutex<Option<PendingUpdatePackage>>,
}

struct PendingUpdatePackage {
    update: Update,
    bytes: Vec<u8>,
}

impl Default for PendingUpdate {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingUpdate {
    pub fn new() -> Self {
        Self {
            info: Mutex::new(None),
            package: Mutex::new(None),
        }
    }

    fn set_info(&self, info: UpdateInfo) {
        if let Ok(mut guard) = self.info.lock() {
            *guard = Some(info);
        }
        if let Ok(mut guard) = self.package.lock() {
            *guard = None;
        }
    }

    fn set_package(&self, update: Update, bytes: Vec<u8>) {
        if let Ok(mut guard) = self.package.lock() {
            *guard = Some(PendingUpdatePackage { update, bytes });
        }
    }

    fn clear(&self) {
        if let Ok(mut guard) = self.info.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.package.lock() {
            *guard = None;
        }
    }
}

fn emit_status(app: &AppHandle, status: UpdateStatus) {
    let _ = app.emit("update-status", status);
}

/// Core update check logic shared by the Tauri command and the silent startup check.
/// The frontend decides how to present the result (dialog vs deferred) based on UI state.
async fn do_check_for_update(
    app: AppHandle,
    settings: &SettingsStore,
) -> Result<Option<UpdateInfo>, String> {
    println!("[Updater] Checking for updates...");
    emit_status(&app, UpdateStatus::Checking);

    let app_settings = settings.get();
    let skipped = app_settings.skipped_update_version.clone();

    let updater = app.updater().map_err(|e| {
        let msg = format!("Failed to get updater: {}", e);
        emit_status(
            &app,
            UpdateStatus::Error {
                message: msg.clone(),
            },
        );
        msg
    })?;

    let check_result =
        tokio::time::timeout(std::time::Duration::from_secs(15), updater.check()).await;

    match check_result {
        Err(_) => {
            let msg = "Update check timed out (15s)".to_string();
            eprintln!("[Updater] {}", msg);
            emit_status(
                &app,
                UpdateStatus::Error {
                    message: msg.clone(),
                },
            );
            Err(msg)
        }
        Ok(Err(e)) => {
            let msg = format!("Update check failed: {}", e);
            eprintln!("[Updater] {}", msg);
            emit_status(
                &app,
                UpdateStatus::Error {
                    message: msg.clone(),
                },
            );
            Err(msg)
        }
        Ok(Ok(Some(update))) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                notes: update.body.clone(),
                date: update.date.map(|d| d.to_string()),
            };

            // Skip if user chose to skip this version
            if !skipped.is_empty() && skipped == info.version {
                println!("[Updater] Skipping v{} (user opted out)", info.version);
                emit_status(&app, UpdateStatus::UpToDate);
                return Ok(None);
            }

            println!(
                "[Updater] Update available: v{} (current: v{})",
                info.version,
                app.package_info().version
            );

            // Store for later download
            if let Some(pending) = app.try_state::<PendingUpdate>() {
                pending.set_info(info.clone());
            }

            emit_status(&app, UpdateStatus::Available { info: info.clone() });

            Ok(Some(info))
        }
        Ok(Ok(None)) => {
            println!("[Updater] Already up to date");
            emit_status(&app, UpdateStatus::UpToDate);
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<Option<UpdateInfo>, String> {
    do_check_for_update(app, &settings).await
}

#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<(), String> {
    println!("[Updater] Downloading update...");

    let updater = app.updater().map_err(|e| {
        let msg = format!("Failed to get updater: {}", e);
        emit_status(
            &app,
            UpdateStatus::Error {
                message: msg.clone(),
            },
        );
        msg
    })?;

    match updater.check().await {
        Ok(Some(update)) => {
            let app_clone = app.clone();
            let mut downloaded_bytes = 0u64;
            let bytes = update
                .download(
                    |chunk_length, total| {
                        downloaded_bytes = downloaded_bytes.saturating_add(chunk_length as u64);
                        let progress = if let Some(t) = total {
                            if t == 0 {
                                0.0
                            } else {
                                ((downloaded_bytes as f64 / t as f64) * 100.0).min(100.0)
                            }
                        } else {
                            0.0
                        };
                        emit_status(&app_clone, UpdateStatus::Downloading { progress, total });
                    },
                    || {
                        println!("[Updater] Download complete");
                        emit_status(&app_clone, UpdateStatus::Downloaded);
                    },
                )
                .await
                .map_err(|e| {
                    let msg = format!("Download failed: {}", e);
                    eprintln!("[Updater] {}", msg);
                    emit_status(
                        &app,
                        UpdateStatus::Error {
                            message: msg.clone(),
                        },
                    );
                    msg
                })?;

            pending.set_package(update, bytes);

            Ok(())
        }
        Ok(None) => {
            let msg = "No update available to download".to_string();
            emit_status(
                &app,
                UpdateStatus::Error {
                    message: msg.clone(),
                },
            );
            Err(msg)
        }
        Err(e) => {
            let msg = format!("Update check failed during download: {}", e);
            emit_status(
                &app,
                UpdateStatus::Error {
                    message: msg.clone(),
                },
            );
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<(), String> {
    println!("[Updater] Installing update and restarting...");
    emit_status(&app, UpdateStatus::Installing);
    let guard = pending
        .package
        .lock()
        .map_err(|_| "Pending update lock poisoned".to_string())?;
    let package = guard
        .as_ref()
        .ok_or_else(|| "No downloaded update is ready to install".to_string())?;
    package.update.install(&package.bytes).map_err(|e| {
        let msg = format!("Install failed: {}", e);
        emit_status(
            &app,
            UpdateStatus::Error {
                message: msg.clone(),
            },
        );
        msg
    })?;
    drop(guard);
    pending.clear();
    app.restart()
}

/// Silent background check used at startup — never returns errors to caller.
pub async fn check_for_update_silent(app: AppHandle) {
    let settings = app.state::<Arc<SettingsStore>>().inner().clone();
    let _ = do_check_for_update(app, &settings).await;
}
