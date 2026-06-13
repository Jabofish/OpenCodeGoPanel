fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_snapshot",
            "refresh_now",
            "get_auth_status",
            "set_visibility",
            "save_cookies",
            "clear_auth",
            "open_login_window",
            "extract_cookies_from_webview",
        ]),
    ))
    .expect("failed to run Tauri build script");
}
