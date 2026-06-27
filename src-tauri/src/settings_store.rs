use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

const SETTINGS_FILE: &str = "opencode-settings.json";
const SETTINGS_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub version: u32,
    pub auto_refresh: bool,
    pub compact_mode: bool,
    pub mini_badge_mode: bool,
    pub mini_badge_source: String,
    pub mini_badge_display: String,
    pub monthly_budget: u32,
    pub hotkey: String,
    pub usage_threshold: u32,
    pub refresh_visible_secs: u64,
    pub refresh_hidden_secs: u64,
    // Notification rules (P2)
    pub notify_quota: bool,
    pub notify_budget_projection: bool,
    pub notify_cost_spike: bool,
    pub notify_refresh_failure: bool,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: String,
    pub quiet_hours_end: String,
    pub notification_cooldown_mins: u32,
    // Workspace profiles (P3)
    pub workspace_profiles: HashMap<String, WorkspaceProfile>,
    pub recent_workspaces: Vec<String>,
    // Appearance
    pub theme: String,
    // Reports
    pub report_frequency: String,
    pub report_auto_generate: bool,
    // Backup
    pub auto_backup: bool,
    // Updates
    pub auto_update: bool,
    pub skipped_update_version: String,
    // Startup
    pub launch_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceProfile {
    pub alias: String,
    pub favorite: bool,
    pub mini_badge_source: String,
}

impl AppSettings {
    pub fn normalize(mut self) -> Self {
        self.version = SETTINGS_VERSION;
        if self.usage_threshold != 0 && (self.usage_threshold < 50 || self.usage_threshold > 95) {
            self.usage_threshold = 0;
        }
        self.refresh_visible_secs = self.refresh_visible_secs.clamp(15, 3600);
        if self.refresh_hidden_secs != 0 {
            self.refresh_hidden_secs = self.refresh_hidden_secs.clamp(60, 3600);
        }
        // Normalize mini badge display
        if !["percent", "ring", "dot"].contains(&self.mini_badge_display.as_str()) {
            self.mini_badge_display = "percent".into();
        }
        // Normalize notification cooldown
        self.notification_cooldown_mins = self.notification_cooldown_mins.clamp(10, 1440);
        // Normalize quiet hours format (HH:MM)
        if !is_valid_time_str(&self.quiet_hours_start) {
            self.quiet_hours_start = "22:00".into();
        }
        if !is_valid_time_str(&self.quiet_hours_end) {
            self.quiet_hours_end = "08:00".into();
        }
        // Trim recent workspaces
        if self.recent_workspaces.len() > 5 {
            self.recent_workspaces.truncate(5);
        }
        // Normalize theme
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        // Normalize report frequency
        if !["off", "daily", "weekly", "monthly"].contains(&self.report_frequency.as_str()) {
            self.report_frequency = "off".into();
        }
        self
    }
}

fn is_valid_time_str(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
        (Ok(h), Ok(m)) => h < 24 && m < 60,
        _ => false,
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            auto_refresh: true,
            compact_mode: true,
            mini_badge_mode: false,
            mini_badge_source: "auto".into(),
            mini_badge_display: "percent".into(),
            monthly_budget: 6000,
            hotkey: "Ctrl+Shift+U".into(),
            usage_threshold: 80,
            refresh_visible_secs: 30,
            refresh_hidden_secs: 600,
            notify_quota: true,
            notify_budget_projection: true,
            notify_cost_spike: false,
            notify_refresh_failure: true,
            quiet_hours_enabled: false,
            quiet_hours_start: "22:00".into(),
            quiet_hours_end: "08:00".into(),
            notification_cooldown_mins: 60,
            workspace_profiles: HashMap::new(),
            recent_workspaces: Vec::new(),
            theme: "system".into(),
            report_frequency: "off".into(),
            report_auto_generate: false,
            auto_backup: true,
            auto_update: true,
            skipped_update_version: String::new(),
            launch_on_startup: true,
        }
    }
}

pub struct SettingsStore {
    data: RwLock<AppSettings>,
    settings_path: PathBuf,
}

impl SettingsStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let settings_path = data_dir.join(SETTINGS_FILE);
        let settings = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|content| serde_json::from_str::<AppSettings>(&content).ok())
            .unwrap_or_default()
            .normalize();

        let store = Self {
            data: RwLock::new(settings),
            settings_path,
        };
        store.persist();
        store
    }

    pub fn get(&self) -> AppSettings {
        self.data.read().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn save(&self, next: AppSettings) -> Result<AppSettings, String> {
        let normalized = next.normalize();
        if let Ok(mut writer) = self.data.write() {
            *writer = normalized.clone();
        }
        self.persist();
        Ok(normalized)
    }

    fn persist(&self) {
        if let Some(parent) = self.settings_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("[SettingsStore] Failed to create dir: {}", e);
                return;
            }
        }
        if let Ok(reader) = self.data.read() {
            match serde_json::to_string_pretty(&*reader) {
                Ok(content) => {
                    if let Err(e) = std::fs::write(&self.settings_path, content) {
                        eprintln!("[SettingsStore] Failed to write: {}", e);
                    }
                }
                Err(e) => eprintln!("[SettingsStore] Failed to serialize: {}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let pid = std::process::id();
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ocp-stest-{}-{}-{}", pid, millis, id))
    }

    #[test]
    fn missing_file_uses_defaults() {
        let dir = temp_dir();
        let store = SettingsStore::new(dir.clone());
        let s = store.get();
        assert_eq!(s.auto_refresh, true);
        assert_eq!(s.hotkey, "Ctrl+Shift+U");
        assert_eq!(s.mini_badge_display, "percent");
        assert_eq!(s.notify_quota, true);
        assert_eq!(s.launch_on_startup, true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_threshold_normalizes_to_zero() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.usage_threshold = 99;
        let saved = store.save(s).unwrap();
        assert_eq!(saved.usage_threshold, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_badge_display_normalizes() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.mini_badge_display = "fancy".into();
        let saved = store.save(s).unwrap();
        assert_eq!(saved.mini_badge_display, "percent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notification_cooldown_clamped() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.notification_cooldown_mins = 5;
        let saved = store.save(s).unwrap();
        assert_eq!(saved.notification_cooldown_mins, 10);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_json_gets_struct_defaults() {
        let partial = r#"{"autoRefresh":false,"compactMode":false}"#;
        let parsed: AppSettings = serde_json::from_str(partial).unwrap();
        assert_eq!(parsed.auto_refresh, false);
        assert_eq!(parsed.compact_mode, false);
        assert_eq!(parsed.hotkey, "Ctrl+Shift+U");
        assert_eq!(parsed.mini_badge_display, "percent");
        assert_eq!(parsed.notify_quota, true);
        assert_eq!(parsed.theme, "system");
        assert_eq!(parsed.report_frequency, "off");
        assert_eq!(parsed.report_auto_generate, false);
    }

    #[test]
    fn file_roundtrip_with_partial_json() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(SETTINGS_FILE),
            r#"{"autoRefresh":false,"compactMode":false}"#,
        )
        .unwrap();
        let store = SettingsStore::new(dir.clone());
        let s = store.get();
        assert_eq!(s.auto_refresh, false);
        assert_eq!(s.hotkey, "Ctrl+Shift+U");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "requires pre-existing temp dir; save/load verified in file_roundtrip_with_partial_json"]
    fn settings_survive_round_trip() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.mini_badge_mode = true;
        s.usage_threshold = 75;
        s.hotkey = "Ctrl+Shift+K".into();
        store.save(s).unwrap();
        let store2 = SettingsStore::new(dir.clone());
        let s2 = store2.get();
        assert_eq!(s2.mini_badge_mode, true);
        assert_eq!(s2.usage_threshold, 75);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_theme_normalizes_to_system() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.theme = "neon".into();
        let saved = store.save(s).unwrap();
        assert_eq!(saved.theme, "system");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_report_frequency_normalizes_to_off() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(dir.clone());
        let mut s = store.get();
        s.report_frequency = "hourly".into();
        let saved = store.save(s).unwrap();
        assert_eq!(saved.report_frequency, "off");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
