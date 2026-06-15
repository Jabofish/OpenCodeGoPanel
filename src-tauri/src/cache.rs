use crate::models::{AppDataSnapshot, WorkspaceEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

const CACHE_FILE: &str = "opencode-cache.json";
const CACHE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedCache {
    #[serde(default = "cache_version")]
    version: u32,
    #[serde(default)]
    active_workspace: String,
    #[serde(default)]
    snapshots: HashMap<String, AppDataSnapshot>,
    #[serde(default)]
    workspaces: Vec<WorkspaceEntry>,
}

impl PersistedCache {
    fn empty() -> Self {
        Self {
            version: CACHE_VERSION,
            active_workspace: String::new(),
            snapshots: HashMap::new(),
            workspaces: Vec::new(),
        }
    }

    fn normalize(mut self) -> Self {
        self.version = CACHE_VERSION;

        if self.active_workspace.is_empty() {
            if let Some(workspace_id) = self
                .snapshots
                .values()
                .find(|snapshot| !snapshot.workspace_id.is_empty())
                .map(|snapshot| snapshot.workspace_id.clone())
            {
                self.active_workspace = workspace_id;
            }
        }

        if self.workspaces.is_empty() {
            if let Some(workspaces) = self
                .snapshots
                .values()
                .find(|snapshot| !snapshot.workspaces.is_empty())
                .map(|snapshot| snapshot.workspaces.clone())
            {
                self.workspaces = workspaces;
            }
        }

        self
    }

    fn from_legacy(snapshot: AppDataSnapshot) -> Self {
        let active_workspace = snapshot.workspace_id.clone();
        let workspaces = snapshot.workspaces.clone();
        let mut snapshots = HashMap::new();

        if !active_workspace.is_empty() || snapshot.error.is_some() {
            snapshots.insert(active_workspace.clone(), snapshot);
        }

        Self {
            version: CACHE_VERSION,
            active_workspace,
            snapshots,
            workspaces,
        }
    }

    fn current_snapshot(&self) -> AppDataSnapshot {
        let snapshot = if !self.active_workspace.is_empty() {
            self.snapshots.get(&self.active_workspace).cloned()
        } else {
            self.snapshots.values().next().cloned()
        };

        let mut snapshot = snapshot.unwrap_or_else(AppDataSnapshot::empty);

        if snapshot.workspace_id.is_empty() {
            snapshot.workspace_id = self.active_workspace.clone();
        }
        if !self.workspaces.is_empty() {
            snapshot.workspaces = self.workspaces.clone();
        }

        snapshot
    }
}

fn cache_version() -> u32 {
    CACHE_VERSION
}

pub struct AppCache {
    state: RwLock<PersistedCache>,
    cache_path: PathBuf,
}

impl AppCache {
    pub fn new(data_dir: PathBuf) -> Self {
        let cache_path = data_dir.join(CACHE_FILE);
        let state = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|content| Self::parse_cache(&content))
            .unwrap_or_else(PersistedCache::empty)
            .normalize();

        Self {
            state: RwLock::new(state),
            cache_path,
        }
    }

    /// Get the active workspace snapshot.
    pub fn get(&self) -> AppDataSnapshot {
        self.state
            .read()
            .map(|reader| reader.current_snapshot())
            .unwrap_or_else(|_| AppDataSnapshot::empty())
    }

    /// Switch the active workspace while keeping any cached data for all workspaces.
    pub fn set_active_workspace(&self, workspace_id: &str) {
        if let Ok(mut writer) = self.state.write() {
            writer.active_workspace = workspace_id.to_string();
            let workspaces = writer.workspaces.clone();

            if !writer.snapshots.contains_key(workspace_id) {
                let mut snapshot = AppDataSnapshot::empty();
                snapshot.workspace_id = workspace_id.to_string();
                snapshot.workspaces = workspaces;
                writer.snapshots.insert(workspace_id.to_string(), snapshot);
            } else if let Some(snapshot) = writer.snapshots.get_mut(workspace_id) {
                if snapshot.workspace_id.is_empty() {
                    snapshot.workspace_id = workspace_id.to_string();
                }
                if snapshot.workspaces.is_empty() {
                    snapshot.workspaces = workspaces;
                }
            }

            self.persist_locked(&writer);
        }
    }

    /// Update the active workspace snapshot.
    pub fn update(&self, snapshot: AppDataSnapshot) {
        if let Ok(mut writer) = self.state.write() {
            let workspace_id = if snapshot.workspace_id.is_empty() {
                writer.active_workspace.clone()
            } else {
                snapshot.workspace_id.clone()
            };

            if !workspace_id.is_empty() {
                writer.active_workspace = workspace_id.clone();
            }
            Self::store_snapshot(&mut writer, workspace_id, snapshot);
            self.persist_locked(&writer);
        }
    }

    /// Mutate the active workspace snapshot in-place and persist it.
    pub fn update_with<F>(&self, update: F)
    where
        F: FnOnce(&mut AppDataSnapshot),
    {
        if let Ok(mut writer) = self.state.write() {
            let workspace_id = writer.active_workspace.clone();
            let key = workspace_id.clone();
            let mut snapshot = writer
                .snapshots
                .remove(&key)
                .unwrap_or_else(AppDataSnapshot::empty);

            if snapshot.workspace_id.is_empty() {
                snapshot.workspace_id = workspace_id.clone();
            }
            if snapshot.workspaces.is_empty() {
                snapshot.workspaces = writer.workspaces.clone();
            }

            update(&mut snapshot);

            let store_key = if !writer.active_workspace.is_empty() {
                writer.active_workspace.clone()
            } else {
                snapshot.workspace_id.clone()
            };
            Self::store_snapshot(&mut writer, store_key, snapshot);
            self.persist_locked(&writer);
        }
    }

    /// Set error state on the active workspace while keeping existing data.
    pub fn set_error(&self, error: String) {
        self.update_with(|snapshot| {
            snapshot.error = Some(error);
        });
    }

    /// Clear cached usage/model data for all workspaces from memory and disk.
    pub fn clear(&self) -> Result<(), String> {
        if let Ok(mut writer) = self.state.write() {
            *writer = PersistedCache::empty();
        }

        if self.cache_path.exists() {
            std::fs::remove_file(&self.cache_path).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn parse_cache(content: &str) -> Option<PersistedCache> {
        let value = serde_json::from_str::<serde_json::Value>(content).ok()?;
        let is_v2 = value.get("snapshots").is_some()
            || value.get("active_workspace").is_some()
            || value.get("version").is_some();

        if is_v2 {
            serde_json::from_value::<PersistedCache>(value).ok()
        } else {
            serde_json::from_value::<AppDataSnapshot>(value)
                .ok()
                .map(PersistedCache::from_legacy)
        }
    }

    fn store_snapshot(
        state: &mut PersistedCache,
        workspace_id: String,
        mut snapshot: AppDataSnapshot,
    ) {
        let key = if workspace_id.is_empty() {
            snapshot.workspace_id.clone()
        } else {
            workspace_id
        };

        if !snapshot.workspaces.is_empty() {
            state.workspaces = snapshot.workspaces.clone();
        } else if !state.workspaces.is_empty() {
            snapshot.workspaces = state.workspaces.clone();
        }

        if snapshot.workspace_id.is_empty() && !key.is_empty() {
            snapshot.workspace_id = key.clone();
        }

        state.snapshots.insert(key, snapshot);
    }

    fn persist_locked(&self, state: &PersistedCache) {
        if let Some(parent) = self.cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("[Cache] Failed to create cache dir: {}", e);
                return;
            }
        }

        match serde_json::to_string_pretty(state) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&self.cache_path, content) {
                    eprintln!("[Cache] Failed to write cache: {}", e);
                }
            }
            Err(e) => eprintln!("[Cache] Failed to serialize cache: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppCache, CACHE_FILE};
    use crate::models::AppDataSnapshot;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_data_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!(
            "opencode-cache-{}-{}-{}",
            name,
            std::process::id(),
            millis
        ))
    }

    #[test]
    fn legacy_single_snapshot_cache_is_migrated() {
        let dir = temp_data_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();

        let mut legacy = AppDataSnapshot::empty();
        legacy.workspace_id = "ws-legacy".into();
        legacy.last_updated = "legacy-time".into();
        legacy.error = None;

        std::fs::write(
            dir.join(CACHE_FILE),
            serde_json::to_string(&legacy).unwrap(),
        )
        .unwrap();

        let cache = AppCache::new(dir.clone());
        let snapshot = cache.get();

        assert_eq!(snapshot.workspace_id, "ws-legacy");
        assert_eq!(snapshot.last_updated, "legacy-time");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn workspace_snapshots_survive_switching_and_reload() {
        let dir = temp_data_dir("workspaces");
        let cache = AppCache::new(dir.clone());

        let mut ws1 = AppDataSnapshot::empty();
        ws1.workspace_id = "ws-1".into();
        ws1.last_updated = "one".into();
        ws1.error = None;
        cache.update(ws1);

        cache.set_active_workspace("ws-2");
        let mut ws2 = AppDataSnapshot::empty();
        ws2.workspace_id = "ws-2".into();
        ws2.last_updated = "two".into();
        ws2.error = None;
        cache.update(ws2);

        cache.set_active_workspace("ws-1");
        assert_eq!(cache.get().last_updated, "one");

        cache.set_active_workspace("ws-2");
        assert_eq!(cache.get().last_updated, "two");

        let reloaded = AppCache::new(dir.clone());
        assert_eq!(reloaded.get().workspace_id, "ws-2");
        assert_eq!(reloaded.get().last_updated, "two");

        let _ = std::fs::remove_dir_all(dir);
    }
}
