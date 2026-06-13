use crate::models::AppDataSnapshot;
use std::sync::RwLock;

pub struct AppCache {
    data: RwLock<AppDataSnapshot>,
}

impl AppCache {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(AppDataSnapshot::empty()),
        }
    }

    /// Get current snapshot (non-blocking read).
    pub fn get(&self) -> AppDataSnapshot {
        self.data.read().unwrap().clone()
    }

    /// Update the cached snapshot.
    pub fn update(&self, snapshot: AppDataSnapshot) {
        if let Ok(mut writer) = self.data.write() {
            *writer = snapshot;
        }
    }

    /// Set error state while keeping existing data.
    pub fn set_error(&self, error: String) {
        if let Ok(mut writer) = self.data.write() {
            writer.error = Some(error);
        }
    }
}
