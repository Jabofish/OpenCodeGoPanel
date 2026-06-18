pub mod auth;
pub mod cache;
pub mod client;
pub mod commands;
pub mod history;
pub mod maintenance;
pub mod models;
pub mod notification_rules;
pub mod paths;
pub mod scheduler;
pub mod settings_store;

use auth::AuthStore;
use cache::AppCache;
use client::OpenCodeClient;
use history::HistoryStore;
use paths::get_data_dir;
use scheduler::RefreshScheduler;
use settings_store::SettingsStore;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

const TRAY_SHOW_ID: &str = "tray-show";
const TRAY_HIDE_ID: &str = "tray-hide";
const DEFAULT_HOTKEY: &str = "Ctrl+Shift+U";

/// Stores the currently registered hotkey string so it can be changed at runtime.
pub struct HotkeyState {
    pub current: Mutex<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[Backend] Starting OpenCode Usage Panel...");

    let data_dir = get_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    let app_cache = Arc::new(AppCache::new(data_dir.clone()));
    println!("[Backend] AppCache created");

    let auth_store = Arc::new(AuthStore::new(data_dir.clone()));
    println!("[Backend] AuthStore created");

    let history_store = Arc::new(HistoryStore::new(data_dir.clone()));
    println!("[Backend] HistoryStore created");

    let settings_store = Arc::new(SettingsStore::new(data_dir.clone()));
    println!("[Backend] SettingsStore created");
    let initial_hotkey = settings_store.get().hotkey;

    let client = Arc::new(OpenCodeClient::new().expect("Failed to create HTTP client"));
    println!("[Backend] HTTP Client created");

    let is_visible = Arc::new(AtomicBool::new(true));
    let scheduler = Arc::new(RefreshScheduler::new(
        client.clone(),
        app_cache.clone(),
        auth_store.clone(),
        history_store.clone(),
        settings_store.clone(),
        is_visible.clone(),
    ));
    println!("[Backend] Scheduler created");

    println!("[Backend] Building Tauri app...");
    let close_scheduler = scheduler.clone();
    tauri::Builder::default()
        .manage(app_cache)
        .manage(auth_store)
        .manage(history_store)
        .manage(settings_store.clone())
        .manage(scheduler.clone())
        .manage(Arc::new(HotkeyState {
            current: Mutex::new(initial_hotkey),
        }))
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh_now,
            commands::get_auth_status,
            commands::set_visibility,
            commands::save_cookies,
            commands::clear_auth,
            commands::clear_cache,
            commands::hide_to_tray,
            commands::set_mini_badge_window,
            commands::open_login_window,
            commands::extract_cookies_from_webview,
            commands::get_history,
            commands::set_hotkey,
            commands::set_threshold,
            commands::get_threshold,
            commands::list_workspaces,
            commands::switch_workspace,
            commands::get_settings,
            commands::save_settings,
            commands::set_refresh_intervals,
            commands::export_data,
            commands::send_test_notification,
            commands::get_local_data_status,
            commands::backup_local_data,
            commands::clear_local_data,
            commands::open_exports_folder,
            commands::run_health_check,
        ])
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_filename("opencode-window-state.json")
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
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
        .setup(move |app| {
            println!("[Backend] Tauri app setup complete");

            // Give the scheduler access to AppHandle for notifications
            scheduler.set_app_handle(app.handle().clone());

            setup_tray(app.handle(), scheduler.clone())?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_skip_taskbar(true);
            }

            // Register global hotkey (default: Ctrl+Shift+U)
            let hotkey_app = app.handle().clone();
            let hotkey_sched = scheduler.clone();
            let hotkey_str = {
                let state = app.state::<Arc<HotkeyState>>();
                state
                    .current
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_else(|_| DEFAULT_HOTKEY.to_string())
            };
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let shortcut = commands::shortcut_from_hotkey(&hotkey_str)
                .or_else(|_| commands::shortcut_from_hotkey(DEFAULT_HOTKEY));
            let cb_app = hotkey_app.clone();
            match shortcut.and_then(|shortcut| {
                hotkey_app
                    .global_shortcut()
                    .on_shortcut(shortcut, move |_app, _event, _shortcut| {
                        println!("[Hotkey] triggered: {}", hotkey_str);
                        toggle_main_window(&cb_app, &hotkey_sched);
                    })
                    .map_err(|e| e.to_string())
            }) {
                Ok(_) => println!("[Backend] Global hotkey registered"),
                Err(e) => eprintln!("[Backend] Failed to register hotkey: {}", e),
            }

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

pub fn toggle_main_window(app: &tauri::AppHandle, scheduler: &RefreshScheduler) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            scheduler.set_visible(false);
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            scheduler.set_visible(true);
        }
    }
}
