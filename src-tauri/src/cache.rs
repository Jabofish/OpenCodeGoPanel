use crate::models::AppDataSnapshot;
use std::path::PathBuf;
use std::sync::RwLock;

const CACHE_FILE: &str = "opencode-cache.json";

pub struct AppCache {
    data: RwLock<AppDataSnapshot>,
    cache_path: PathBuf,
}

impl AppCache {
    pub fn new(data_dir: PathBuf) -> Self {
        let cache_path = data_dir.join(CACHE_FILE);
        let data = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|content| serde_json::from_str::<AppDataSnapshot>(&content).ok())
            .unwrap_or_else(AppDataSnapshot::empty);

        Self {
            data: RwLock::new(data),
            cache_path,
        }
    }

    /// Get current snapshot (non-blocking read).
    pub fn get(&self) -> AppDataSnapshot {
        self.data
            .read()
            .map(|reader| reader.clone())
            .unwrap_or_else(|_| AppDataSnapshot::empty())
    }

    /// Update the cached snapshot.
    pub fn update(&self, snapshot: AppDataSnapshot) {
        if let Ok(mut writer) = self.data.write() {
            *writer = snapshot;
            self.persist_locked(&writer);
        }
    }

    /// Mutate the cached snapshot in-place and persist it.
    pub fn update_with<F>(&self, update: F)
    where
        F: FnOnce(&mut AppDataSnapshot),
    {
        if let Ok(mut writer) = self.data.write() {
            update(&mut writer);
            self.persist_locked(&writer);
        }
    }

    /// Set error state while keeping existing data.
    pub fn set_error(&self, error: String) {
        if let Ok(mut writer) = self.data.write() {
            writer.error = Some(error);
            self.persist_locked(&writer);
        }
    }

    /// Clear cached usage/model data from memory and disk.
    pub fn clear(&self) -> Result<(), String> {
        if let Ok(mut writer) = self.data.write() {
            *writer = AppDataSnapshot::empty();
        }

        if self.cache_path.exists() {
            std::fs::remove_file(&self.cache_path).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn persist_locked(&self, snapshot: &AppDataSnapshot) {
        if let Some(parent) = self.cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("[Cache] Failed to create cache dir: {}", e);
                return;
            }
        }

        match serde_json::to_string(snapshot) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&self.cache_path, content) {
                    eprintln!("[Cache] Failed to write cache: {}", e);
                }
            }
            Err(e) => eprintln!("[Cache] Failed to serialize cache: {}", e),
        }
    }
}
