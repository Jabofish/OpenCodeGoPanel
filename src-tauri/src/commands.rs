use crate::account::{AccountInfo, AccountsManager};
use crate::auth::{AuthStore, CookieEntry, WorkspaceInfo};
use crate::cache::AppCache;
use crate::client::OpenCodeClient;
use crate::history::HistoryStore;
use crate::maintenance::{self, ClearLocalDataEffect};
use crate::models::{AppDataSnapshot, HealthCheck, HistoryEntry, LocalDataStatus, WorkspaceEntry};
use crate::paths;
use crate::scheduler::RefreshScheduler;
use crate::settings_store::{AppSettings, SettingsStore};
use crate::HotkeyState;
use chrono::Utc;
use serde::Deserialize;
use std::{collections::HashSet, sync::Arc};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Url, WebviewUrl, WebviewWindowBuilder};
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
        window
            .set_max_size(None::<LogicalSize<f64>>)
            .map_err(|e| e.to_string())?;
        window
            .set_min_size(Some(PANEL_MIN_SIZE))
            .map_err(|e| e.to_string())?;
        window.set_resizable(true).map_err(|e| e.to_string())?;
        window.set_shadow(false).map_err(|e| e.to_string())?;
        window.set_size(PANEL_SIZE).map_err(|e| e.to_string())?;
    } else {
        window.set_resizable(false).map_err(|e| e.to_string())?;
        window.set_shadow(false).map_err(|e| e.to_string())?;
        window
            .set_min_size(Some(MINI_BADGE_SIZE))
            .map_err(|e| e.to_string())?;
        window
            .set_size(MINI_BADGE_SIZE)
            .map_err(|e| e.to_string())?;
        window
            .set_max_size(Some(MINI_BADGE_SIZE))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum LoginWindowOpenMode {
    FocusExisting,
    CreateCleanSession,
}

fn login_window_open_mode(login_window_exists: bool) -> LoginWindowOpenMode {
    if login_window_exists {
        LoginWindowOpenMode::FocusExisting
    } else {
        LoginWindowOpenMode::CreateCleanSession
    }
}

#[tauri::command]
pub async fn open_login_window(app: AppHandle) -> Result<(), String> {
    println!("[Command] open_login_window called");

    let existing_login_window = app.get_webview_window("login");
    let open_mode = login_window_open_mode(existing_login_window.is_some());

    if let Some(login_window) = existing_login_window {
        if open_mode == LoginWindowOpenMode::FocusExisting {
            println!("[Command] Login window already exists, focusing it");
            let _ = login_window.show();
            let _ = login_window.set_focus();
            return Ok(());
        }
    }

    if open_mode == LoginWindowOpenMode::CreateCleanSession {
        clear_opencode_webview_cookies(&app)?;
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

fn clear_opencode_webview_cookies(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app
        .get_webview_window("main")
        .or_else(|| app.get_webview_window("login"))
    else {
        println!("[Command] No webview window available for cookie cleanup");
        return Ok(());
    };

    let mut seen = HashSet::new();
    let mut deleted = 0usize;
    for url in opencode_cookie_urls()? {
        match window.cookies_for_url(url) {
            Ok(cookies) => {
                for cookie in cookies {
                    if !should_clear_opencode_cookie_domain(cookie.domain()) {
                        continue;
                    }
                    let key = (
                        cookie.name().to_string(),
                        cookie.domain().unwrap_or("").to_string(),
                        cookie.path().unwrap_or("").to_string(),
                    );
                    if !seen.insert(key) {
                        continue;
                    }
                    window.delete_cookie(cookie).map_err(|e| e.to_string())?;
                    deleted += 1;
                }
            }
            Err(e) => {
                println!("[Command] Failed to read OpenCode webview cookies: {}", e);
            }
        }
    }
    println!("[Command] Cleared {} OpenCode webview cookies", deleted);
    Ok(())
}

fn opencode_cookie_urls() -> Result<Vec<Url>, String> {
    ["https://opencode.ai/", "https://opencode.ai/auth"]
        .into_iter()
        .map(|url| Url::parse(url).map_err(|e| format!("Failed to parse OpenCode URL: {}", e)))
        .collect()
}

fn should_clear_opencode_cookie_domain(domain: Option<&str>) -> bool {
    let Some(domain) = domain else {
        return true;
    };
    let normalized = domain.trim().trim_start_matches('.').to_ascii_lowercase();
    normalized == "opencode.ai" || normalized.ends_with(".opencode.ai")
}

#[tauri::command]
pub async fn extract_cookies_from_webview(
    app: AppHandle,
    cookies_json: String,
    workspace_id: String,
    auth: tauri::State<'_, Arc<AuthStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    client: tauri::State<'_, Arc<OpenCodeClient>>,
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
    auth.save_cookies(cookies.clone(), workspace_id.clone())
        .map_err(|e| format!("Failed to save cookies: {}", e))?;

    seed_authenticated_workspace_cache(&cache, &auth, &workspace_id);
    if let Err(e) = refresh_login_usage_cache(&client, &cache, &cookies, &workspace_id).await {
        println!("[Command] Login usage prefetch failed: {}", e);
    }

    println!(
        "[Command] Cookies saved successfully for workspace: {}",
        workspace_id
    );

    // Trigger immediate refresh to load data
    println!("[Command] Triggering immediate refresh...");
    scheduler.refresh_now().await;

    // Notify main window that auth state changed (login succeeded)
    let _ = app.emit(
        "auth-state-changed",
        serde_json::json!({ "state": "logged_in" }),
    );

    // Close login window
    if let Some(login_window) = app.get_webview_window("login") {
        login_window.close().map_err(|e| e.to_string())?;
        println!("[Command] Login window closed");
    }

    Ok(true)
}

fn seed_authenticated_workspace_cache(cache: &AppCache, auth: &AuthStore, workspace_id: &str) {
    cache.set_active_workspace(workspace_id);
    let auth_workspaces = workspace_entries_from_auth(auth.list_workspaces());
    cache.update_with(|snapshot| {
        snapshot.workspace_id = workspace_id.to_string();
        snapshot.error = None;
        if !auth_workspaces.is_empty() {
            snapshot.workspaces = auth_workspaces;
        } else if snapshot.workspaces.iter().all(|ws| ws.id != workspace_id) {
            snapshot.workspaces.push(WorkspaceEntry {
                id: workspace_id.to_string(),
                name: workspace_id.to_string(),
                slug: None,
            });
        }
        snapshot.refresh_state.last_error = None;
    });
}

fn workspace_entries_from_auth(workspaces: Vec<WorkspaceInfo>) -> Vec<WorkspaceEntry> {
    workspaces
        .into_iter()
        .map(|workspace| WorkspaceEntry {
            id: workspace.workspace_id,
            name: workspace.display_name,
            slug: None,
        })
        .collect()
}

async fn refresh_login_usage_cache(
    client: &OpenCodeClient,
    cache: &AppCache,
    cookies: &[CookieEntry],
    workspace_id: &str,
) -> Result<(), String> {
    let (usage, workspaces) = client.fetch_usage(cookies, workspace_id).await?;
    cache.update_with(|snapshot| {
        snapshot.workspace_id = workspace_id.to_string();
        snapshot.usage = usage;
        if !workspaces.is_empty() {
            snapshot.workspaces = workspaces;
        }
        snapshot.error = None;
        snapshot.last_updated = Utc::now().to_rfc3339();
        snapshot.refresh_state.last_error = None;
    });
    Ok(())
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
    let new_shortcut = shortcut_from_hotkey(&hotkey)?;
    let old_hotkey = hotkey_state
        .current
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| "Ctrl+Shift+U".to_string());

    if let Ok(old_shortcut) = shortcut_from_hotkey(&old_hotkey) {
        let _ = app.global_shortcut().unregister(old_shortcut);
    }

    let sched = scheduler.inner().clone();
    let toggle_app = app.clone();
    app.global_shortcut()
        .on_shortcut(new_shortcut, move |_app, _event, _shortcut| {
            crate::toggle_main_window(&toggle_app, &sched);
        })
        .map_err(|e| format!("Failed to register hotkey: {}", e))?;

    if let Ok(mut current) = hotkey_state.current.lock() {
        *current = hotkey.clone();
    }
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
pub async fn list_accounts(
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<Vec<AccountInfo>, String> {
    let settings = settings.get();
    let mut accounts = settings.accounts.clone();
    accounts.sort_by(|a, b| {
        let a_active = a.id == settings.active_account_id;
        let b_active = b.id == settings.active_account_id;
        b_active
            .cmp(&a_active)
            .then_with(|| b.last_used_at.cmp(&a.last_used_at))
    });
    Ok(accounts)
}

#[tauri::command]
pub async fn add_account(
    settings: tauri::State<'_, Arc<SettingsStore>>,
    auth: tauri::State<'_, Arc<AuthStore>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    display_name: Option<String>,
) -> Result<AccountInfo, String> {
    let current = settings.get();
    let mut manager = AccountsManager::new(current.accounts.clone(), current.active_account_id);
    let sanitized_name = display_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let added = manager.add(sanitized_name).clone();
    let (accounts, active) = manager.into_parts();

    settings.save_account_index(accounts, active.clone())?;
    let account_dir = settings.accounts_root().join(&active);
    std::fs::create_dir_all(&account_dir).map_err(|e| format!("create account dir: {}", e))?;

    auth.set_active_account(&active);
    history.set_active_account(account_dir.join(crate::history::HISTORY_FILE))?;
    cache.set_active_account(account_dir.join(crate::cache::CACHE_FILE))?;

    Ok(added)
}

#[tauri::command]
pub async fn rename_account(
    settings: tauri::State<'_, Arc<SettingsStore>>,
    account_id: String,
    new_name: String,
) -> Result<(), String> {
    let current = settings.get();
    let mut manager = AccountsManager::new(current.accounts.clone(), current.active_account_id);
    manager.rename(&account_id, new_name.trim().to_string())?;
    let (accounts, active) = manager.into_parts();
    settings.save_account_index(accounts, active)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_account(
    settings: tauri::State<'_, Arc<SettingsStore>>,
    auth: tauri::State<'_, Arc<AuthStore>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
    account_id: String,
) -> Result<(), String> {
    let current = settings.get();
    let was_active = current.active_account_id == account_id;
    let mut manager = AccountsManager::new(current.accounts.clone(), current.active_account_id);
    manager.remove(&account_id)?;
    let (accounts, active) = manager.into_parts();
    settings.save_account_index(accounts, active.clone())?;

    if was_active {
        let account_dir = settings.accounts_root().join(&active);
        auth.set_active_account(&active);
        history.set_active_account(account_dir.join(crate::history::HISTORY_FILE))?;
        cache.set_active_account(account_dir.join(crate::cache::CACHE_FILE))?;
        if let Some(auth_data) = auth.load_auth() {
            if !auth_data.active_workspace.is_empty() {
                cache.set_active_workspace(&auth_data.active_workspace);
            }
        }
        let scheduler = scheduler.inner().clone();
        tauri::async_runtime::spawn(async move {
            scheduler.refresh_now().await;
        });
    }

    let removed_dir = settings.accounts_root().join(&account_id);
    if removed_dir.exists() {
        std::fs::remove_dir_all(&removed_dir).map_err(|e| format!("remove account dir: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn switch_account(
    account_id: String,
    auth: tauri::State<'_, Arc<AuthStore>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
    settings: tauri::State<'_, Arc<SettingsStore>>,
    scheduler: tauri::State<'_, Arc<RefreshScheduler>>,
) -> Result<(), String> {
    settings.set_active_account(&account_id)?;

    let account_dir = settings.accounts_root().join(&account_id);
    auth.set_active_account(&account_id);
    history.set_active_account(account_dir.join(crate::history::HISTORY_FILE))?;
    cache.set_active_account(account_dir.join(crate::cache::CACHE_FILE))?;

    if let Some(auth_data) = auth.load_auth() {
        if !auth_data.active_workspace.is_empty() {
            cache.set_active_workspace(&auth_data.active_workspace);
        }
    }

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
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable()
            .map_err(|e| format!("Failed to enable autostart: {}", e))?;
    } else {
        mgr.disable()
            .map_err(|e| format!("Failed to disable autostart: {}", e))?;
    }
    println!(
        "[Autostart] {}: {}",
        if enabled { "enabled" } else { "disabled" },
        mgr.is_enabled().unwrap_or(false)
    );
    Ok(())
}

#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Failed to query autostart: {}", e))
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
            let mut csv = String::with_capacity(160 + snapshot.usage_records.len() * 256);
            csv.push_str(
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
                csv.push_str(
                    &r.cache_write_5m_tokens
                        .map_or(String::new(), |v| v.to_string()),
                );
                csv.push(',');
                csv.push_str(
                    &r.cache_write_1h_tokens
                        .map_or(String::new(), |v| v.to_string()),
                );
                csv.push(',');
                csv.push_str(&r.cost.to_string());
                csv.push(',');
                csv.push_str(&csv_cell(&r.key_id));
                csv.push(',');
                csv.push_str(&csv_cell(&r.session_id));
                csv.push(',');
                csv.push_str(
                    &r.enrichment
                        .as_ref()
                        .and_then(|e| e.plan.as_ref())
                        .map_or(String::new(), csv_cell),
                );
                csv.push('\n');
            }
            std::fs::write(&path, csv).map_err(|e| e.to_string())?;
            Ok(path.to_string_lossy().into_owned())
        }
        "daily-costs-csv" => {
            let path = export_dir.join(format!("opencode-daily-costs-{}.csv", ts));
            let mut csv = String::with_capacity(40 + snapshot.daily_costs.len() * 96);
            csv.push_str("date,model,total_cost,key_id,plan\n");
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
        _ => Err(format!(
            "Unknown export kind: {}. Use snapshot-json, usage-records-csv, or daily-costs-csv",
            kind
        )),
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

/// Parse a hotkey string such as `Ctrl+Shift+U` or `Alt+Space` into a Shortcut.
///
/// Accepts any combination of modifiers (Ctrl/Control, Shift, Alt/Option,
/// Super/Meta/Win/Cmd/Command) plus exactly one key (A-Z, 0-9, F1-F12, Space,
/// arrows, Home/End, etc.). Tokens are case-insensitive and order-insensitive.
/// Returns the parsed Shortcut, or an error message on failure.
pub(crate) fn shortcut_from_hotkey(hotkey: &str) -> Result<Shortcut, String> {
    let (mods, code) = parse_hotkey(hotkey)?;
    Ok(Shortcut::new(Some(mods), code))
}

fn parse_hotkey(hotkey: &str) -> Result<(Modifiers, Code), String> {
    let mut mods = Modifiers::empty();
    let mut key: Option<Code> = None;

    for raw in hotkey.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(m) = parse_modifier_token(token) {
            mods |= m;
            continue;
        }
        if let Some(c) = parse_key_token(token) {
            if key.is_some() {
                return Err(format!(
                    "Hotkey must contain exactly one key; '{}' is a second key",
                    token
                ));
            }
            key = Some(c);
            continue;
        }
        return Err(format!("Unknown hotkey token: '{}'", token));
    }

    let code = key.ok_or_else(|| "Hotkey must contain a key (e.g. Ctrl+Shift+U)".to_string())?;
    if mods.is_empty() {
        return Err("Hotkey must include at least one modifier (Ctrl/Shift/Alt/Super)".into());
    }
    Ok((mods, code))
}

/// Map a modifier token (case-insensitive) to a Modifiers flag, or None.
fn parse_modifier_token(token: &str) -> Option<Modifiers> {
    match token.trim().to_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CONTROL),
        "shift" => Some(Modifiers::SHIFT),
        "alt" | "option" | "opt" => Some(Modifiers::ALT),
        "super" | "meta" | "win" | "cmd" | "command" => Some(Modifiers::SUPER),
        _ => None,
    }
}

// Lookup tables for single-character keys. `Code` has no `from(u8)`, so we
// index these arrays instead of relying on enum discriminant arithmetic.
const LETTER_KEYS: [Code; 26] = [
    Code::KeyA,
    Code::KeyB,
    Code::KeyC,
    Code::KeyD,
    Code::KeyE,
    Code::KeyF,
    Code::KeyG,
    Code::KeyH,
    Code::KeyI,
    Code::KeyJ,
    Code::KeyK,
    Code::KeyL,
    Code::KeyM,
    Code::KeyN,
    Code::KeyO,
    Code::KeyP,
    Code::KeyQ,
    Code::KeyR,
    Code::KeyS,
    Code::KeyT,
    Code::KeyU,
    Code::KeyV,
    Code::KeyW,
    Code::KeyX,
    Code::KeyY,
    Code::KeyZ,
];
const DIGIT_KEYS: [Code; 10] = [
    Code::Digit0,
    Code::Digit1,
    Code::Digit2,
    Code::Digit3,
    Code::Digit4,
    Code::Digit5,
    Code::Digit6,
    Code::Digit7,
    Code::Digit8,
    Code::Digit9,
];
const FUNCTION_KEYS: [Code; 12] = [
    Code::F1,
    Code::F2,
    Code::F3,
    Code::F4,
    Code::F5,
    Code::F6,
    Code::F7,
    Code::F8,
    Code::F9,
    Code::F10,
    Code::F11,
    Code::F12,
];

/// Map a key token (case-insensitive) to a Code, or None. Accepts both the
/// bare form (`A`, `0`, `Space`, `F1`, `Up`) and the canonical form
/// (`KeyA`, `Digit0`, `ArrowUp`) used by the underlying `keyboard_types` crate.
fn parse_key_token(token: &str) -> Option<Code> {
    let upper = token.trim().to_uppercase();
    if upper.is_empty() {
        return None;
    }
    let mut chars = upper.chars();
    let first = chars.next()?;
    let rest_len = upper.len() - first.len_utf8();

    // Bare single character: letter or digit.
    if rest_len == 0 {
        if first.is_ascii_uppercase() {
            return Some(LETTER_KEYS[(first as u8 - b'A') as usize]);
        }
        if first.is_ascii_digit() {
            return Some(DIGIT_KEYS[(first as u8 - b'0') as usize]);
        }
    }

    // Canonical letter/digit forms: KeyA, Digit0.
    if let Some(suffix) = upper.strip_prefix("KEY") {
        if let Some(c) = suffix.chars().next() {
            if suffix.len() == 1 && c.is_ascii_uppercase() {
                return Some(LETTER_KEYS[(c as u8 - b'A') as usize]);
            }
        }
    }
    if let Some(suffix) = upper.strip_prefix("DIGIT") {
        if let Some(c) = suffix.chars().next() {
            if suffix.len() == 1 && c.is_ascii_digit() {
                return Some(DIGIT_KEYS[(c as u8 - b'0') as usize]);
            }
        }
    }

    // Function keys F1-F12 (and F13-F24 if the crate ever needs them).
    if first == 'F' {
        if let Ok(n) = upper[1..].parse::<usize>() {
            if (1..=FUNCTION_KEYS.len()).contains(&n) {
                return Some(FUNCTION_KEYS[n - 1]);
            }
        }
    }

    Some(match upper.as_str() {
        "SPACE" => Code::Space,
        "UP" | "ARROWUP" => Code::ArrowUp,
        "DOWN" | "ARROWDOWN" => Code::ArrowDown,
        "LEFT" | "ARROWLEFT" => Code::ArrowLeft,
        "RIGHT" | "ARROWRIGHT" => Code::ArrowRight,
        "HOME" => Code::Home,
        "END" => Code::End,
        "PAGEUP" | "PGUP" => Code::PageUp,
        "PAGEDOWN" | "PGDN" => Code::PageDown,
        "INSERT" | "INS" => Code::Insert,
        "DELETE" | "DEL" => Code::Delete,
        "BACKSPACE" | "BACK" => Code::Backspace,
        "ENTER" | "RETURN" => Code::Enter,
        "TAB" => Code::Tab,
        "ESCAPE" | "ESC" => Code::Escape,
        "CAPSLOCK" | "CAPS" => Code::CapsLock,
        _ => return None,
    })
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
pub async fn send_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_local_data_status(
    _history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<LocalDataStatus, String> {
    Ok(maintenance::local_data_status())
}

#[tauri::command]
pub async fn backup_local_data(
    cache: tauri::State<'_, Arc<AppCache>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<String, String> {
    maintenance::backup_local_data(settings.get(), history.get_entries(90), cache.get())
}

#[tauri::command]
pub async fn clear_local_data(
    cache: tauri::State<'_, Arc<AppCache>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    scope: String,
) -> Result<(), String> {
    match maintenance::clear_local_data(&scope)? {
        ClearLocalDataEffect::ClearCache => cache.clear()?,
        ClearLocalDataEffect::ClearHistory => history.clear(),
        ClearLocalDataEffect::None => {}
    }
    Ok(())
}

#[tauri::command]
pub async fn open_exports_folder() -> Result<String, String> {
    maintenance::open_exports_folder()
}

#[tauri::command]
pub async fn run_health_check(
    auth: tauri::State<'_, Arc<AuthStore>>,
    cache: tauri::State<'_, Arc<AppCache>>,
) -> Result<HealthCheck, String> {
    let snapshot = cache.get();
    Ok(maintenance::run_health_check(
        auth.has_valid_cookies(),
        snapshot.error,
    ))
}

#[tauri::command]
pub async fn generate_report(
    period: String,
    cache: tauri::State<'_, Arc<AppCache>>,
    history: tauri::State<'_, Arc<HistoryStore>>,
    settings: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<String, String> {
    let snapshot = cache.get();
    let history_entries = history.get_entries(90);
    let app_settings = settings.get();
    let data_dir = paths::get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    crate::report_generator::generate_usage_report(
        &snapshot,
        &history_entries,
        &app_settings,
        &period,
        &data_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_legacy_default_still_works() {
        let (mods, code) = parse_hotkey("Ctrl+Shift+U").unwrap();
        assert_eq!(code, Code::KeyU);
        assert!(mods.contains(Modifiers::CONTROL));
        assert!(mods.contains(Modifiers::SHIFT));
        assert!(shortcut_from_hotkey("Ctrl+Shift+K").is_ok());
    }

    #[test]
    fn parse_alt_p() {
        let (mods, code) = parse_hotkey("Alt+P").unwrap();
        assert_eq!(code, Code::KeyP);
        assert!(mods.contains(Modifiers::ALT));
        assert!(!mods.contains(Modifiers::CONTROL));
    }

    #[test]
    fn parse_ctrl_space() {
        let (mods, code) = parse_hotkey("Ctrl+Space").unwrap();
        assert_eq!(code, Code::Space);
        assert!(mods.contains(Modifiers::CONTROL));
    }

    #[test]
    fn opencode_cookie_domain_matching_is_exact_or_subdomain_only() {
        assert!(should_clear_opencode_cookie_domain(None));
        assert!(should_clear_opencode_cookie_domain(Some("opencode.ai")));
        assert!(should_clear_opencode_cookie_domain(Some(".opencode.ai")));
        assert!(should_clear_opencode_cookie_domain(Some(
            "auth.opencode.ai"
        )));
        assert!(!should_clear_opencode_cookie_domain(Some(
            "evilopencode.ai"
        )));
        assert!(!should_clear_opencode_cookie_domain(Some(
            "opencode.ai.evil.test"
        )));
    }

    #[test]
    fn existing_login_window_is_focused_instead_of_recreated() {
        assert_eq!(
            login_window_open_mode(true),
            LoginWindowOpenMode::FocusExisting
        );
        assert_eq!(
            login_window_open_mode(false),
            LoginWindowOpenMode::CreateCleanSession
        );
    }

    #[test]
    fn auth_workspaces_can_seed_snapshot_workspace_entries() {
        let entries = workspace_entries_from_auth(vec![WorkspaceInfo {
            workspace_id: "wrk_123".to_string(),
            display_name: "Default".to_string(),
            is_active: true,
        }]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "wrk_123");
        assert_eq!(entries[0].name, "Default");
        assert_eq!(entries[0].slug, None);
    }

    #[test]
    fn parse_super_u() {
        let (mods, code) = parse_hotkey("Super+U").unwrap();
        assert_eq!(code, Code::KeyU);
        assert!(mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn parse_ctrl_shift_alt_k() {
        let (mods, code) = parse_hotkey("Ctrl+Shift+Alt+K").unwrap();
        assert_eq!(code, Code::KeyK);
        assert!(mods.contains(Modifiers::CONTROL));
        assert!(mods.contains(Modifiers::SHIFT));
        assert!(mods.contains(Modifiers::ALT));
    }

    #[test]
    fn parse_is_order_insensitive_and_case_insensitive() {
        // Reversed order, mixed case, alternate modifier spellings.
        let (mods, code) = parse_hotkey("k+shift+Control").unwrap();
        assert_eq!(code, Code::KeyK);
        assert!(mods.contains(Modifiers::CONTROL));
        assert!(mods.contains(Modifiers::SHIFT));

        let (mods, code) = parse_hotkey("p+opt").unwrap();
        assert_eq!(code, Code::KeyP);
        assert!(mods.contains(Modifiers::ALT));

        let (mods, code) = parse_hotkey("win+f1").unwrap();
        assert_eq!(code, Code::F1);
        assert!(mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn parse_digits_and_canonical_forms() {
        let (_, code) = parse_hotkey("Ctrl+5").unwrap();
        assert_eq!(code, Code::Digit5);

        let (_, code) = parse_hotkey("Ctrl+Digit0").unwrap();
        assert_eq!(code, Code::Digit0);

        let (_, code) = parse_hotkey("Ctrl+KeyZ").unwrap();
        assert_eq!(code, Code::KeyZ);

        let (_, code) = parse_hotkey("Ctrl+F12").unwrap();
        assert_eq!(code, Code::F12);
    }

    #[test]
    fn parse_arrow_and_nav_keys() {
        assert_eq!(parse_hotkey("Ctrl+Up").unwrap().1, Code::ArrowUp);
        assert_eq!(parse_hotkey("Ctrl+ArrowDown").unwrap().1, Code::ArrowDown);
        assert_eq!(parse_hotkey("Ctrl+Home").unwrap().1, Code::Home);
        assert_eq!(parse_hotkey("Ctrl+PageUp").unwrap().1, Code::PageUp);
    }

    #[test]
    fn rejects_bare_key_without_modifier() {
        // A key alone is not a valid global shortcut.
        assert!(parse_hotkey("U").is_err());
        assert!(parse_hotkey("A").is_err());
        assert!(parse_hotkey("Space").is_err());
        assert!(parse_hotkey("F5").is_err());
    }

    #[test]
    fn rejects_two_keys() {
        assert!(parse_hotkey("Ctrl+A+B").is_err());
        assert!(parse_hotkey("Space+Ctrl+A").is_err());
    }

    #[test]
    fn rejects_unknown_token_and_garbage() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("foo").is_err());
        assert!(parse_hotkey("Ctrl+Foo").is_err());
        assert!(parse_hotkey("Ctrl+Shift++").is_err());
        assert!(parse_hotkey("Ctrl+").is_err());
    }

    /// Verify that every Tauri command registered in build.rs has a matching
    /// `allow-*` permission in at least one capability JSON file.
    /// This catches the common mistake of adding a command but forgetting to
    /// grant the frontend permission to call it.
    #[test]
    fn all_commands_have_capability_permissions() {
        // All commands registered in build.rs (must stay in sync).
        let commands: &[&str] = &[
            "get_snapshot",
            "refresh_now",
            "get_auth_status",
            "set_visibility",
            "save_cookies",
            "clear_auth",
            "clear_cache",
            "hide_to_tray",
            "set_mini_badge_window",
            "open_login_window",
            "extract_cookies_from_webview",
            "get_history",
            "set_hotkey",
            "set_threshold",
            "get_threshold",
            "list_workspaces",
            "switch_workspace",
            "list_accounts",
            "add_account",
            "rename_account",
            "remove_account",
            "switch_account",
            "get_settings",
            "save_settings",
            "set_refresh_intervals",
            "export_data",
            "send_test_notification",
            "send_notification",
            "get_local_data_status",
            "backup_local_data",
            "clear_local_data",
            "open_exports_folder",
            "run_health_check",
            "generate_report",
            "set_autostart",
            "get_autostart",
            "check_for_update",
            "download_update",
            "install_update",
        ];

        // Read all capability JSON files and collect allow-* permissions.
        let caps_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut all_permissions: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&caps_dir).expect("capabilities dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            let cap: serde_json::Value = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e));
            if let Some(perms) = cap["permissions"].as_array() {
                for p in perms {
                    if let Some(s) = p.as_str() {
                        all_permissions.push(s.to_string());
                    }
                }
            }
        }

        // Convert snake_case command name to expected permission string.
        fn command_to_permission(cmd: &str) -> String {
            format!("allow-{}", cmd.replace('_', "-"))
        }

        let mut missing: Vec<String> = Vec::new();
        for &cmd in commands {
            let perm = command_to_permission(cmd);
            if !all_permissions.contains(&perm) {
                missing.push(format!("{} → {}", cmd, perm));
            }
        }

        assert!(
            missing.is_empty(),
            "Commands missing from capability permissions:\n  {}\n\
             Add the corresponding allow-* entries to capabilities/*.json",
            missing.join("\n  ")
        );
    }
}
