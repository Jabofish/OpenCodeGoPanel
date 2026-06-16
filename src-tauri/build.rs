fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
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
            "get_settings",
            "save_settings",
            "set_refresh_intervals",
            "export_data",
        ]),
    ))
    .expect("failed to run Tauri build script");
}
