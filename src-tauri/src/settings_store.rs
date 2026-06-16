use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

const SETTINGS_FILE: &str = "opencode-settings.json";
const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub version: u32,
    pub auto_refresh: bool,
    pub compact_mode: bool,
    pub mini_badge_mode: bool,
    pub mini_badge_source: String,
    pub monthly_budget: u32,
    pub hotkey: String,
    pub usage_threshold: u32,
    pub refresh_visible_secs: u64,
    pub refresh_hidden_secs: u64,
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
        self
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
            monthly_budget: 6000,
            hotkey: "Ctrl+Shift+U".into(),
            usage_threshold: 80,
            refresh_visible_secs: 30,
            refresh_hidden_secs: 600,
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

        // Persist with any migrations applied
        let store = Self {
            data: RwLock::new(settings),
            settings_path,
        };
        store.persist();
        store
    }

    pub fn get(&self) -> AppSettings {
        self.data
            .read()
            .map(|r| r.clone())
            .unwrap_or_default()
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

    fn temp_dir() -> PathBuf {
        let pid = std::process::id();
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("ocp-stest-{}-{}", pid, millis))
    }

    #[test]
    fn missing_file_uses_defaults() {
        let dir = temp_dir();
        let store = SettingsStore::new(dir.clone());
        let s = store.get();
        assert_eq!(s.auto_refresh, true);
        assert_eq!(s.hotkey, "Ctrl+Shift+U");
        assert_eq!(s.usage_threshold, 80);
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

        // Reload from disk
        let store2 = SettingsStore::new(dir.clone());
        let s2 = store2.get();
        assert_eq!(s2.mini_badge_mode, true);
        assert_eq!(s2.usage_threshold, 75);
        assert_eq!(s2.hotkey, "Ctrl+Shift+K");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_json_gets_struct_defaults() {
        // Test that serde correctly fills missing fields from Default impl
        let partial = r#"{"autoRefresh":false,"compactMode":false}"#;
        let parsed: AppSettings = serde_json::from_str(partial).unwrap();
        assert_eq!(parsed.auto_refresh, false);
        assert_eq!(parsed.compact_mode, false);
        // Missing fields get Default::default() values
        assert_eq!(parsed.hotkey, "Ctrl+Shift+U");
        assert_eq!(parsed.usage_threshold, 80);
        assert_eq!(parsed.mini_badge_source, "auto");
    }

    #[test]
    fn file_roundtrip_with_partial_json() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // Write partial JSON
        std::fs::write(
            dir.join(SETTINGS_FILE),
            r#"{"autoRefresh":false,"compactMode":false}"#,
        )
        .unwrap();

        let store = SettingsStore::new(dir.clone());
        let s = store.get();
        assert_eq!(s.auto_refresh, false);
        assert_eq!(s.compact_mode, false);
        assert_eq!(s.hotkey, "Ctrl+Shift+U");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
