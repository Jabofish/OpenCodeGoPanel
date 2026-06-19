use crate::settings_store::SettingsStore;
use serde::Serialize;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

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

    let app_settings = settings.get();
    let skipped = app_settings.skipped_update_version.clone();

    let updater = app.updater().map_err(|e| {
        let msg = format!("Failed to get updater: {}", e);
        emit_status(&app, UpdateStatus::Error { message: msg.clone() });
        msg
    })?;

    match updater.check().await {
        Ok(Some(update)) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                notes: update.body.clone(),
                date: update.date.map(|d| d.to_string()),
            };

            // Skip if user chose to skip this version
            if !skipped.is_empty() && skipped == info.version {
                println!(
                    "[Updater] Skipping v{} (user opted out)",
                    info.version
                );
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
                if let Ok(mut guard) = pending.info.lock() {
                    *guard = Some(info.clone());
                }
            }

            emit_status(&app, UpdateStatus::Available { info: info.clone() });

            Ok(Some(info))
        }
        Ok(None) => {
            println!("[Updater] Already up to date");
            emit_status(&app, UpdateStatus::UpToDate);
            Ok(None)
        }
        Err(e) => {
            let msg = format!("Update check failed: {}", e);
            eprintln!("[Updater] {}", msg);
            emit_status(&app, UpdateStatus::Error { message: msg.clone() });
            Err(msg)
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
pub async fn download_update(app: AppHandle) -> Result<(), String> {
    println!("[Updater] Downloading update...");

    let updater = app.updater().map_err(|e| {
        let msg = format!("Failed to get updater: {}", e);
        emit_status(&app, UpdateStatus::Error { message: msg.clone() });
        msg
    })?;

    match updater.check().await {
        Ok(Some(update)) => {
            let app_clone = app.clone();
            update
                .download_and_install(
                    |chunk_length, total| {
                        let progress = if let Some(t) = total {
                            (chunk_length as f64 / t as f64) * 100.0
                        } else {
                            0.0
                        };
                        emit_status(
                            &app_clone,
                            UpdateStatus::Downloading {
                                progress,
                                total,
                            },
                        );
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
                    emit_status(&app, UpdateStatus::Error { message: msg.clone() });
                    msg
                })?;

            // Clear pending state after successful download
            if let Some(pending) = app.try_state::<PendingUpdate>() {
                if let Ok(mut guard) = pending.info.lock() {
                    *guard = None;
                }
            }

            Ok(())
        }
        Ok(None) => {
            let msg = "No update available to download".to_string();
            emit_status(&app, UpdateStatus::Error { message: msg.clone() });
            Err(msg)
        }
        Err(e) => {
            let msg = format!("Update check failed during download: {}", e);
            emit_status(&app, UpdateStatus::Error { message: msg.clone() });
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    println!("[Updater] Installing update and restarting...");
    emit_status(&app, UpdateStatus::Installing);
    app.restart();
}

/// Silent background check used at startup — never returns errors to caller.
pub async fn check_for_update_silent(app: AppHandle) {
    let settings = app.state::<Arc<SettingsStore>>().inner().clone();
    let _ = do_check_for_update(app, &settings).await;
}
