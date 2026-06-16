use crate::auth::{AuthStore, CookieEntry, WorkspaceInfo};
use crate::cache::AppCache;
use crate::history::HistoryStore;
use crate::models::{AppDataSnapshot, HistoryEntry, LocalDataStatus, HealthCheck};
use crate::paths;
use crate::scheduler::RefreshScheduler;
use crate::settings_store::{AppSettings, SettingsStore};
use crate::HotkeyState;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, LogicalSize, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_notification::NotificationExt;

#[tauri::command]
pub async fn get_snapshot(
    cache: tauri::State<'_, Arc<AppCache>>,
) -> Result<AppDataSnapshot, String> {
    Ok(cache.get())
}

#[tauri::command]
pub async fn refresh_now(scheduler: tauri::State<'_, Arc<RefreshScheduler>>) -> Result<(), String> {
    scheduler.refresh_now().await;
    Ok(())
}

#[tauri::command]
pub async fn get_auth_status(auth: tauri::State<'_, Arc<AuthStore>>) -> Result<bool, String> {
    Ok(auth.has_valid_cookies())
}

#[tauri::command]
pub async fn set_visibility(
    visible: bool,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
) -> Result<(), String> {
    scheduler.set_visible(visible);
    Ok(())
}

#[tauri::command]
pub async fn save_cookies(
    cookies: Vec<CookieEntry>,
    workspace_id: String,
    auth: tauri::State<'_, Arc<AuthStore>>,
) -> Result<(), String> {
    auth.save_cookies(cookies, workspace_id)
}

#[tauri::command]
pub async fn clear_auth(auth: tauri::State<'_, Arc<AuthStore>>) -> Result<(), String> {
    auth.clear_cookies()
}

#[tauri::command]
pub async fn clear_cache(cache: tauri::State<'_, Arc<AppCache>>) -> Result<(), String> {
    println!("[Command] clear_cache called");
    cache.clear()
}

#[tauri::command]
pub async fn hide_to_tray(
    app: AppHandle,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
) -> Result<(), String> {
    println!("[Command] hide_to_tray called");
    scheduler.set_visible(false);
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_mini_badge_window(app: AppHandle, expanded: bool) -> Result<(), String> {
    const MINI_BADGE_SIZE: LogicalSize<f64> = LogicalSize {
        width: 60.0,
        height: 60.0,
    };
    const PANEL_MIN_SIZE: LogicalSize<f64> = LogicalSize {
        width: 280.0,
        height: 320.0,
    };
    const PANEL_SIZE: LogicalSize<f64> = LogicalSize {
        width: 320.0,
        height: 480.0,
    };

    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };

    if expanded {
        window.set_max_size(None::<LogicalSize<f64>>).map_err(|e| e.to_string())?;
        window.set_min_size(Some(PANEL_MIN_SIZE)).map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;
        window.set_shadow(false).map_err(|e| e.to_string())?;
        window.set_size(PANEL_SIZE).map_err(|e| e.to_string())?;
    } else {
        window.set_resizable(false).map_err(|e| e.to_string())?;
        window.set_shadow(false).map_err(|e| e.to_string())?;
        window.set_min_size(Some(MINI_BADGE_SIZE)).map_err(|e| e.to_string())?;
        window.set_size(MINI_BADGE_SIZE).map_err(|e| e.to_string())?;
        window.set_max_size(Some(MINI_BADGE_SIZE)).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn open_login_window(app: AppHandle) -> Result<(), String> {
    println!("[Command] open_login_window called");

    // Check if login window already exists
    if app.get_webview_window("login").is_some() {
        println!("[Command] Login window already exists");
        return Ok(());
    }

    println!("[Command] Creating login window...");

    WebviewWindowBuilder::new(
        &app,
        "login",
        WebviewUrl::External("https://opencode.ai/auth".parse().unwrap()),
    )
    .title("Login to OpenCode")
    .inner_size(1000.0, 700.0)
    .center()
    .resizable(true)
    .initialization_script(include_str!("../../src/js/login-helper.js"))
    .build()
    .map_err(|e| {
        let err_msg = format!("Failed to create login window: {}", e);
        println!("[Command] Error: {}", err_msg);
        err_msg
    })?;

    println!("[Command] Login window created successfully with helper script injected");

    Ok(())
}

#[tauri::command]
pub async fn extract_cookies_from_webview(
    app: AppHandle,
    cookies_json: String,
    workspace_id: String,
    auth: tauri::State<'_, Arc<AuthStore>>,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
) -> Result<bool, String> {
    println!("[Command] extract_cookies_from_webview called");
    println!("[Command] Received workspace_id: {}", workspace_id);

    // Parse cookies from JSON
    #[derive(Deserialize)]
    struct JsCookie {
        name: String,
        value: String,
    }

    let js_cookies: Vec<JsCookie> = serde_json::from_str(&cookies_json)
        .map_err(|e| format!("Failed to parse cookies: {}", e))?;

    if workspace_id.is_empty() {
        return Err("Workspace ID not found. Please navigate to your workspace page.".to_string());
    }

    println!("[Command] Extracted {} document cookies", js_cookies.len());

    let mut cookies = Vec::new();
    if let Some(login_window) = app.get_webview_window("login") {
        let url = Url::parse("https://opencode.ai/")
            .map_err(|e| format!("Failed to parse OpenCode URL: {}", e))?;
        match login_window.cookies_for_url(url) {
            Ok(webview_cookies) => {
                println!(
                    "[Command] Extracted {} webview cookies",
                    webview_cookies.len()
                );
                cookies = webview_cookies
                    .into_iter()
                    .map(|c| CookieEntry {
                        name: c.name().to_string(),
                        value: c.value().to_string(),
                        domain: c.domain().unwrap_or(".opencode.ai").to_string(),
                        path: c.path().unwrap_or("/").to_string(),
                    })
                    .collect();
            }
            Err(e) => {
                println!("[Command] Failed to read webview cookies: {}", e);
            }
        }
    }

    if cookies.is_empty() {
        cookies = js_cookies
            .into_iter()
            .map(|c| CookieEntry {
                name: c.name,
                value: c.value,
                domain: ".opencode.ai".to_string(),
                path: "/".to_string(),
            })
            .collect();
    }

    if cookies.is_empty() {
        return Err("No valid cookies found. Please make sure you're logged in.".to_string());
    }

    // Save cookies
    auth.save_cookies(cookies, workspace_id.clone())
        .map_err(|e| format!("Failed to save cookies: {}", e))?;

    println!(
        "[Command] Cookies saved successfully for workspace: {}",
        workspace_id
    );

    // Trigger immediate refresh to load data
    println!("[Command] Triggering immediate refresh...");
    scheduler.refresh_now().await;

    // Close login window
    if let Some(login_window) = app.get_webview_window("login") {
        login_window.close().map_err(|e| e.to_string())?;
        println!("[Command] Login window closed");
    }

    Ok(true)
}

#[tauri::command]
pub async fn get_history(
    history: tauri::State<'_, Arc<HistoryStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    days: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    let snapshot = cache.get();
    let workspace_id = if snapshot.workspace_id.is_empty() {
        None
    } else {
        Some(snapshot.workspace_id.as_str())
    };
    Ok(history.get_entries_for_workspace(days.unwrap_or(90), workspace_id))
}

#[tauri::command]
pub async fn set_hotkey(
    app: AppHandle,
    hotkey_state: tauri::State<'_, Arc<HotkeyState>>,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
    hotkey: String,
) -> Result<(), String> {
    // Unregister old shortcut
    let old_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyU);
    let _ = app.global_shortcut().unregister(old_shortcut);

    let code = parse_ctrl_shift_letter_hotkey(&hotkey)?;

    let new_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), code);
    let sched = scheduler.inner().clone();
    let toggle_app = app.clone();
    app.global_shortcut()
        .on_shortcut(new_shortcut, move |_app, _event, _shortcut| {
            crate::toggle_main_window(&toggle_app, &sched);
        })
        .map_err(|e| format!("Failed to register hotkey: {}", e))?;

    *hotkey_state.current.lock().unwrap() = hotkey.clone();
    println!("[Hotkey] Changed to: {}", hotkey);
    Ok(())
}

#[tauri::command]
pub async fn set_threshold(
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
    threshold: u32,
) -> Result<(), String> {
    if threshold != 0 && !(50..=95).contains(&threshold) {
        return Err("Threshold must be 0 (disabled) or between 50 and 95".into());
    }
    scheduler.set_threshold(threshold);
    println!("[Threshold] Set to: {}", threshold);
    Ok(())
}

#[tauri::command]
pub async fn get_threshold(
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
) -> Result<u32, String> {
    Ok(scheduler.get_threshold())
}

#[tauri::command]
pub async fn list_workspaces(
    auth: tauri::State<'_, Arc<AuthStore>>,
) -> Result<Vec<WorkspaceInfo>, String> {
    Ok(auth.list_workspaces())
}

#[tauri::command]
pub async fn switch_workspace(
    auth: tauri::State<'_, Arc<AuthStore>>,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    workspace_id: String,
) -> Result<(), String> {
    // Persist the active workspace before refreshing; the scheduler reads it from auth.
    auth.switch_workspace(&workspace_id)?;

    // Show any persisted data for this workspace immediately, then refresh in the background.
    cache.set_active_workspace(&workspace_id);

    let scheduler = scheduler.inner().clone();
    tauri::async_runtime::spawn(async move {
        scheduler.refresh_now().await;
    });
    Ok(())
}

#[tauri::command]
pub async fn get_settings(
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<AppSettings, String> {
    Ok(settings.get())
}

#[tauri::command]
pub async fn save_settings(
    settings: tauri::State<'_, Arc<SettingsStore>>,
    next: AppSettings,
) -> Result<AppSettings, String> {
    let normalized = settings.save(next)?;
    Ok(normalized)
}

#[tauri::command]
pub async fn set_refresh_intervals(
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
    visible_secs: u64,
    hidden_secs: u64,
) -> Result<(), String> {
    scheduler.set_refresh_intervals(visible_secs, hidden_secs);
    println!(
        "[Scheduler] Intervals updated: visible={}s hidden={}s",
        visible_secs, hidden_secs
    );
    Ok(())
}

#[tauri::command]
pub async fn export_data(
    cache: tauri::State<'_, Arc<AppCache>>,
    kind: String,
) -> Result<String, String> {
    let snapshot = cache.get();
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let export_dir = data_dir.join("exports");
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S");

    match kind.as_str() {
        "snapshot-json" => {
            let path = export_dir.join(format!("opencode-snapshot-{}.json", ts));
            let json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
            std::fs::write(&path, json).map_err(|e| e.to_string())?;
            Ok(path.to_string_lossy().into_owned())
        }
        "usage-records-csv" => {
            let path = export_dir.join(format!("opencode-usage-records-{}.csv", ts));
            let mut csv = String::from(
                "id,workspace_id,time_created,model,provider,input_tokens,output_tokens,\
                reasoning_tokens,cache_read_tokens,cache_write_5m_tokens,cache_write_1h_tokens,\
                cost,key_id,session_id,plan\n",
            );
            for r in &snapshot.usage_records {
                csv.push_str(&csv_cell(&r.id));
                csv.push(',');
                csv.push_str(&csv_cell(&r.workspace_id));
                csv.push(',');
                csv.push_str(&csv_cell(&r.time_created));
                csv.push(',');
                csv.push_str(&csv_cell(&r.model));
                csv.push(',');
                csv.push_str(&csv_cell(&r.provider));
                csv.push(',');
                csv.push_str(&r.input_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.output_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.reasoning_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.cache_read_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.cache_write_5m_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.cache_write_1h_tokens.map_or(String::new(), |v| v.to_string()));
                csv.push(',');
                csv.push_str(&r.cost.to_string());
                csv.push(',');
                csv.push_str(&csv_cell(&r.key_id));
                csv.push(',');
                csv.push_str(&csv_cell(&r.session_id));
                csv.push(',');
                csv.push_str(&r.enrichment.as_ref().and_then(|e| e.plan.as_ref()).map_or(String::new(), csv_cell));
                csv.push('\n');
            }
            std::fs::write(&path, csv).map_err(|e| e.to_string())?;
            Ok(path.to_string_lossy().into_owned())
        }
        "daily-costs-csv" => {
            let path = export_dir.join(format!("opencode-daily-costs-{}.csv", ts));
            let mut csv = String::from("date,model,total_cost,key_id,plan\n");
            for d in &snapshot.daily_costs {
                csv.push_str(&csv_cell(&d.date));
                csv.push(',');
                csv.push_str(&csv_cell(&d.model));
                csv.push(',');
                csv.push_str(&d.total_cost.to_string());
                csv.push(',');
                csv.push_str(&csv_cell(&d.key_id));
                csv.push(',');
                csv.push_str(&d.plan.as_ref().map_or(String::new(), csv_cell));
                csv.push('\n');
            }
            std::fs::write(&path, csv).map_err(|e| e.to_string())?;
            Ok(path.to_string_lossy().into_owned())
        }
        _ => Err(format!("Unknown export kind: {}. Use snapshot-json, usage-records-csv, or daily-costs-csv", kind)),
    }
}

fn csv_cell(value: impl AsRef<str>) -> String {
    let s = value.as_ref();
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Parse a Ctrl+Shift+<letter> hotkey string into a key Code.
/// Returns the Code on success, or an error message on failure.
fn parse_ctrl_shift_letter_hotkey(hotkey: &str) -> Result<Code, String> {
    let parts: Vec<&str> = hotkey.split('+').collect();
    if parts.len() != 3 {
        return Err("Invalid hotkey format. Use Ctrl+Shift+<key>".into());
    }
    // Validate modifier keys (case-insensitive)
    let ctrl = parts[0].trim().to_lowercase();
    let shift = parts[1].trim().to_lowercase();
    if ctrl != "ctrl" || shift != "shift" {
        return Err("Invalid modifiers. Use Ctrl+Shift+<key>".into());
    }
    let key_char = parts[2].trim().to_uppercase();
    if key_char.len() != 1 || !key_char.chars().next().unwrap().is_ascii_alphabetic() {
        return Err("Hotkey key must be a single letter A-Z".into());
    }
    match key_char.as_str() {
        "A" => Ok(Code::KeyA),
        "B" => Ok(Code::KeyB),
        "C" => Ok(Code::KeyC),
        "D" => Ok(Code::KeyD),
        "E" => Ok(Code::KeyE),
        "F" => Ok(Code::KeyF),
        "G" => Ok(Code::KeyG),
        "H" => Ok(Code::KeyH),
        "I" => Ok(Code::KeyI),
        "J" => Ok(Code::KeyJ),
        "K" => Ok(Code::KeyK),
        "L" => Ok(Code::KeyL),
        "M" => Ok(Code::KeyM),
        "N" => Ok(Code::KeyN),
        "O" => Ok(Code::KeyO),
        "P" => Ok(Code::KeyP),
        "Q" => Ok(Code::KeyQ),
        "R" => Ok(Code::KeyR),
        "S" => Ok(Code::KeyS),
        "T" => Ok(Code::KeyT),
        "U" => Ok(Code::KeyU),
        "V" => Ok(Code::KeyV),
        "W" => Ok(Code::KeyW),
        "X" => Ok(Code::KeyX),
        "Y" => Ok(Code::KeyY),
        "Z" => Ok(Code::KeyZ),
        _ => Err("Unsupported key".into()),
    }
}

#[tauri::command]
pub async fn send_test_notification(app: AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("OpenCode Usage")
        .body("Notifications are working.")
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_local_data_status(
    _history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<LocalDataStatus, String> {
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let file_bytes = |name: &str| -> u64 {
        std::fs::metadata(data_dir.join(name))
            .map(|m| m.len())
            .unwrap_or(0)
    };

    let mut export_bytes = 0u64;
    let mut export_count = 0u32;
    let export_dir = data_dir.join("exports");
    if let Ok(entries) = std::fs::read_dir(&export_dir) {
        for entry in entries.flatten() {
            if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
                export_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                export_count += 1;
            }
        }
    }

    Ok(LocalDataStatus {
        data_dir: data_dir.to_string_lossy().into_owned(),
        cache_bytes: file_bytes("opencode-cache.json"),
        history_bytes: file_bytes("opencode-history.json"),
        settings_bytes: file_bytes("opencode-settings.json"),
        auth_bytes: file_bytes("opencode-auth.json"),
        export_bytes,
        export_count,
    })
}

#[tauri::command]
pub async fn backup_local_data(
    cache: tauri::State<'_, Arc<AppCache>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<String, String> {
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let export_dir = data_dir.join("exports");
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let path = export_dir.join(format!("opencode-backup-{}.json", ts));

    let backup = serde_json::json!({
        "version": 1,
        "createdAt": Utc::now().to_rfc3339(),
        "settings": settings.get(),
        "history": history.get_entries(90),
        "cache": cache.get(),
        "auth": null,
    });

    std::fs::write(&path, serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn clear_local_data(
    cache: tauri::State<'_, Arc<AppCache>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    scope: String,
) -> Result<(), String> {
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    match scope.as_str() {
        "cache" => cache.clear()?,
        "history" => {
            let hf = data_dir.join("opencode-history.json");
            let _ = std::fs::remove_file(&hf);
            history.clear();
        }
        "exports" => {
            let export_dir = data_dir.join("exports");
            if export_dir.exists() {
                for entry in std::fs::read_dir(&export_dir).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
                        std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        "settings" => {
            let sf = data_dir.join("opencode-settings.json");
            let _ = std::fs::remove_file(&sf);
        }
        _ => {
            return Err(format!(
                "Unknown scope: {}. Use cache, history, exports, or settings.",
                scope
            ))
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn open_exports_folder() -> Result<String, String> {
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let export_dir = data_dir.join("exports");
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;

    let path_str = export_dir.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&export_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&export_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&export_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(path_str)
}

#[tauri::command]
pub async fn run_health_check(
    auth: tauri::State<'_, Arc<AuthStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
) -> Result<HealthCheck, String> {
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let snapshot = cache.get();
    let history_ok = std::fs::metadata(data_dir.join("opencode-history.json"))
        .map(|m| m.is_file())
        .unwrap_or(true);

    Ok(HealthCheck {
        has_auth: auth.has_valid_cookies(),
        cache_ok: true,
        settings_ok: true,
        history_ok,
        data_dir: data_dir.to_string_lossy().into_owned(),
        last_refresh_error: snapshot.error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_ctrl_shift_letter() {
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+Shift+U").is_ok());
        assert!(parse_ctrl_shift_letter_hotkey("ctrl+shift+a").is_ok());
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+Shift+Z").is_ok());
    }

    #[test]
    fn rejects_alt_instead_of_ctrl() {
        assert!(parse_ctrl_shift_letter_hotkey("Alt+Shift+U").is_err());
    }

    #[test]
    fn rejects_digit_key() {
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+Shift+1").is_err());
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+Shift+9").is_err());
    }

    #[test]
    fn rejects_missing_modifier() {
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+U").is_err());
        assert!(parse_ctrl_shift_letter_hotkey("Shift+U").is_err());
    }

    #[test]
    fn rejects_empty_and_garbage() {
        assert!(parse_ctrl_shift_letter_hotkey("").is_err());
        assert!(parse_ctrl_shift_letter_hotkey("foo").is_err());
        assert!(parse_ctrl_shift_letter_hotkey("Ctrl+Shift++").is_err());
    }
}
