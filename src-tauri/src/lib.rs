pub mod auth;
pub mod cache;
pub mod client;
pub mod commands;
pub mod models;
pub mod scheduler;

use auth::AuthStore;
use cache::AppCache;
use client::OpenCodeClient;
use scheduler::RefreshScheduler;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

const TRAY_SHOW_ID: &str = "tray-show";
const TRAY_HIDE_ID: &str = "tray-hide";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[Backend] Starting OpenCode Usage Panel...");

    let data_dir = get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    let app_cache = Arc::new(AppCache::new(data_dir.clone()));
    println!("[Backend] AppCache created");

    let auth_store = Arc::new(AuthStore::new(data_dir));
    println!("[Backend] AuthStore created");

    let client = Arc::new(OpenCodeClient::new().expect("Failed to create HTTP client"));
    println!("[Backend] HTTP Client created");

    let is_visible = Arc::new(AtomicBool::new(true));
    let scheduler = Arc::new(RefreshScheduler::new(
        client.clone(),
        app_cache.clone(),
        auth_store.clone(),
        is_visible.clone(),
    ));
    println!("[Backend] Scheduler created");

    println!("[Backend] Building Tauri app...");
    let close_scheduler = scheduler.clone();
    tauri::Builder::default()
        .manage(app_cache)
        .manage(auth_store)
        .manage(scheduler.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh_now,
            commands::get_auth_status,
            commands::set_visibility,
            commands::save_cookies,
            commands::clear_auth,
            commands::clear_cache,
            commands::hide_to_tray,
            commands::open_login_window,
            commands::extract_cookies_from_webview,
        ])
        .on_window_event(move |window, event| {
            // Only intercept close for the main window; login window closes normally.
            // This makes Alt+F4 / window-X hide to tray instead of quitting the app.
            // The tray menu's Quit item still exits via PredefinedMenuItem::quit.
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    close_scheduler.set_visible(false);
                }
            }
        })
        .setup(move |_app| {
            println!("[Backend] Tauri app setup complete");
            println!("[Backend] Main window should be visible now");

            setup_tray(_app.handle(), scheduler.clone())?;

            // Start the refresh scheduler now that the runtime is fully initialized
            let sched = scheduler.clone();
            tauri::async_runtime::spawn(async move {
                println!("[Backend] Starting scheduler...");
                sched.start_adaptive().await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to launch app");
}

fn setup_tray(app: &tauri::AppHandle, scheduler: Arc<RefreshScheduler>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "Show", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, TRAY_HIDE_ID, "Hide to tray", true, None::<&str>)?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &hide, &separator, &quit])?;

    let tray_scheduler = scheduler.clone();
    let menu_scheduler = scheduler.clone();

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("OpenCode Usage")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app, &menu_scheduler),
            TRAY_HIDE_ID => hide_main_window(app, &menu_scheduler),
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle(), &tray_scheduler);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle, scheduler: &RefreshScheduler) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        scheduler.set_visible(true);
    }
}

fn hide_main_window(app: &tauri::AppHandle, scheduler: &RefreshScheduler) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        scheduler.set_visible(false);
    }
}

/// Resolve the OS-specific app data directory for storing cookies.
fn get_data_dir() -> Option<std::path::PathBuf> {
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
