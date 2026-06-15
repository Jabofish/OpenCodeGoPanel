use crate::auth::{AuthStore, CookieEntry, WorkspaceInfo};
use crate::cache::AppCache;
use crate::history::HistoryStore;
use crate::models::{AppDataSnapshot, HistoryEntry};
use crate::scheduler::RefreshScheduler;
use crate::HotkeyState;
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, LogicalSize, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

#[tauri::command]
pub async fn get_snapshot(
    cache: tauri::State<'_, Arc<AppCache>>,
) -> Result<AppDataSnapshot, String> {
    println!("[Command] get_snapshot called");
    Ok(cache.get())
}

#[tauri::command]
pub async fn refresh_now(scheduler: tauri::State<'_, Arc<RefreshScheduler>>) -> Result<(), String> {
    println!("[Command] refresh_now called");
    scheduler.refresh_now().await;
    Ok(())
}

#[tauri::command]
pub async fn get_auth_status(auth: tauri::State<'_, Arc<AuthStore>>) -> Result<bool, String> {
    println!("[Command] get_auth_status called");
    let has_auth = auth.has_valid_cookies();
    println!("[Command] has_valid_cookies: {}", has_auth);
    Ok(has_auth)
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
    days: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    Ok(history.get_entries(days.unwrap_or(90)))
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

    // Parse new hotkey string (simplified: only supports Ctrl+Shift+<key>)
    let parts: Vec<&str> = hotkey.split('+').collect();
    if parts.len() != 3 {
        return Err("Invalid hotkey format. Use Ctrl+Shift+<key>".into());
    }
    let key_char = parts[2].trim().to_uppercase();
    if key_char.len() != 1 || !key_char.chars().next().unwrap().is_ascii_alphabetic() {
        return Err("Hotkey key must be a single letter A-Z".into());
    }
    let code = match key_char.as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        _ => return Err("Unsupported key".into()),
    };

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
    if threshold != 0 && (threshold < 50 || threshold > 95) {
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
