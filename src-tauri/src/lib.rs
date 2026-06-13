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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("[Backend] Starting OpenCode Usage Panel...");

    let app_cache = Arc::new(AppCache::new());
    println!("[Backend] AppCache created");

    let auth_store = Arc::new(AuthStore::new(
        get_data_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
    ));
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
            commands::open_login_window,
            commands::extract_cookies_from_webview,
        ])
        .setup(move |_app| {
            println!("[Backend] Tauri app setup complete");
            println!("[Backend] Main window should be visible now");

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
