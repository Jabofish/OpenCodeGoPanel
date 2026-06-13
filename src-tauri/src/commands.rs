use crate::auth::{AuthStore, CookieEntry};
use crate::cache::AppCache;
use crate::models::AppDataSnapshot;
use crate::scheduler::RefreshScheduler;
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

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
