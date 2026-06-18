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
        match self.state.write() {
            Ok(mut writer) => {
                writer.active_workspace = workspace_id.to_string();

                if !writer.snapshots.contains_key(workspace_id) {
                    let mut snapshot = AppDataSnapshot::empty();
                    snapshot.workspace_id = workspace_id.to_string();
                    snapshot.workspaces = writer.workspaces.clone();
                    writer.snapshots.insert(workspace_id.to_string(), snapshot);
                } else {
                    let needs_workspace_list = writer
                        .snapshots
                        .get(workspace_id)
                        .map(|snapshot| snapshot.workspaces.is_empty())
                        .unwrap_or(false);
                    let workspaces = if needs_workspace_list && !writer.workspaces.is_empty() {
                        Some(writer.workspaces.clone())
                    } else {
                        None
                    };

                    if let Some(snapshot) = writer.snapshots.get_mut(workspace_id) {
                        if snapshot.workspace_id.is_empty() {
                            snapshot.workspace_id = workspace_id.to_string();
                        }
                        if let Some(workspaces) = workspaces {
                            snapshot.workspaces = workspaces;
                        }
                    }
                }

                let _ = self.persist_locked(&writer);
            }
            Err(_) => eprintln!("[Cache] Cache lock poisoned while switching workspace"),
        }
    }

    /// Update the active workspace snapshot.
    pub fn update(&self, snapshot: AppDataSnapshot) {
        match self.state.write() {
            Ok(mut writer) => {
                let workspace_id = if snapshot.workspace_id.is_empty() {
                    writer.active_workspace.clone()
                } else {
                    snapshot.workspace_id.clone()
                };

                if !workspace_id.is_empty() {
                    writer.active_workspace = workspace_id.clone();
                }
                Self::store_snapshot(&mut writer, workspace_id, snapshot);
                let _ = self.persist_locked(&writer);
            }
            Err(_) => eprintln!("[Cache] Cache lock poisoned while updating snapshot"),
        }
    }

    /// Mutate the active workspace snapshot in-place and persist it.
    pub fn update_with<F>(&self, update: F)
    where
        F: FnOnce(&mut AppDataSnapshot),
    {
        match self.state.write() {
            Ok(mut writer) => {
                let workspace_id = writer.active_workspace.clone();
                let mut snapshot = writer
                    .snapshots
                    .remove(&workspace_id)
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
                let _ = self.persist_locked(&writer);
            }
            Err(_) => eprintln!("[Cache] Cache lock poisoned while mutating snapshot"),
        }
    }

    /// Set error state on the active workspace while keeping existing data.
    pub fn set_error(&self, error: String) {
        self.update_with(|snapshot| {
            snapshot.error = Some(error);
        });
    }

    /// Update just the refresh_state field on the active workspace snapshot.
    pub fn update_refresh_state<F>(&self, f: F)
    where
        F: FnOnce(&mut crate::models::RefreshState),
    {
        self.update_with(|snapshot| {
            f(&mut snapshot.refresh_state);
        });
    }

    /// Clear cached usage/model data for all workspaces from memory and disk.
    /// Preserves workspace list and active workspace ID to avoid state loss.
    pub fn clear(&self) -> Result<(), String> {
        let mut writer = self
            .state
            .write()
            .map_err(|_| "Cache lock poisoned while clearing cache".to_string())?;
        let active_workspace = writer.active_workspace.clone();
        let workspaces = writer.workspaces.clone();

        *writer = PersistedCache::empty();
        writer.active_workspace = active_workspace;
        writer.workspaces = workspaces;

        self.persist_locked(&writer)
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

    fn persist_locked(&self, state: &PersistedCache) -> Result<(), String> {
        if let Some(parent) = self.cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let message = format!("Failed to create cache dir {}: {}", parent.display(), e);
                eprintln!("[Cache] {}", message);
                return Err(message);
            }
        }

        match serde_json::to_string_pretty(state) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&self.cache_path, content) {
                    let message = format!(
                        "Failed to write cache file {}: {}",
                        self.cache_path.display(),
                        e
                    );
                    eprintln!("[Cache] {}", message);
                    Err(message)
                } else {
                    Ok(())
                }
            }
            Err(e) => {
                let message = format!("Failed to serialize cache: {}", e);
                eprintln!("[Cache] {}", message);
                Err(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppCache, CACHE_FILE};
    use crate::models::{AppDataSnapshot, WorkspaceEntry};
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

    #[test]
    fn clear_preserves_active_workspace_and_workspace_list() {
        let dir = temp_data_dir("clear");
        let cache = AppCache::new(dir.clone());

        let workspace = WorkspaceEntry {
            id: "ws-1".into(),
            name: "Primary".into(),
            slug: Some("primary".into()),
        };
        let mut snapshot = AppDataSnapshot::empty();
        snapshot.workspace_id = workspace.id.clone();
        snapshot.workspaces = vec![workspace.clone()];
        snapshot.last_updated = "before-clear".into();
        snapshot.error = None;
        cache.update(snapshot);

        cache.clear().unwrap();
        let cleared = cache.get();

        assert_eq!(cleared.workspace_id, "ws-1");
        assert_eq!(cleared.workspaces.len(), 1);
        assert_eq!(cleared.workspaces[0].id, workspace.id);
        assert_eq!(cleared.last_updated, "");
        assert_eq!(cleared.error.as_deref(), Some("Not yet loaded"));

        let reloaded = AppCache::new(dir.clone());
        let reloaded_snapshot = reloaded.get();
        assert_eq!(reloaded_snapshot.workspace_id, "ws-1");
        assert_eq!(reloaded_snapshot.workspaces.len(), 1);
        assert_eq!(reloaded_snapshot.workspaces[0].name, "Primary");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn update_with_can_seed_first_workspace() {
        let dir = temp_data_dir("update-with-seed");
        let cache = AppCache::new(dir.clone());

        cache.update_with(|snapshot| {
            snapshot.workspace_id = "ws-seeded".into();
            snapshot.last_updated = "seeded".into();
            snapshot.error = None;
        });

        let seeded = cache.get();
        assert_eq!(seeded.workspace_id, "ws-seeded");
        assert_eq!(seeded.last_updated, "seeded");

        let reloaded = AppCache::new(dir.clone());
        let reloaded_snapshot = reloaded.get();
        assert_eq!(reloaded_snapshot.workspace_id, "ws-seeded");
        assert_eq!(reloaded_snapshot.last_updated, "seeded");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn v2_cache_without_active_workspace_normalizes_from_snapshot() {
        let dir = temp_data_dir("normalize-v2");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(CACHE_FILE),
            r#"{
                "version": 1,
                "active_workspace": "",
                "workspaces": [],
                "snapshots": {
                    "ws-normalized": {
                        "usage": {
                            "rolling": { "status": "unknown", "usage_percent": 0, "reset_in_sec": 0 },
                            "weekly": { "status": "unknown", "usage_percent": 0, "reset_in_sec": 0 },
                            "monthly": { "status": "unknown", "usage_percent": 0, "reset_in_sec": 0 }
                        },
                        "model_calls": { "models": [], "total_calls": 0 },
                        "workspace_id": "ws-normalized",
                        "last_updated": "normalized",
                        "error": null,
                        "usage_records": [],
                        "daily_costs": [],
                        "workspaces": [
                            { "id": "ws-normalized", "name": "Normalized", "slug": null }
                        ],
                        "refresh_state": {
                            "is_refreshing": false,
                            "phase": "idle",
                            "last_started_at": null,
                            "last_finished_at": null,
                            "last_error": null
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let cache = AppCache::new(dir.clone());
        let snapshot = cache.get();

        assert_eq!(snapshot.workspace_id, "ws-normalized");
        assert_eq!(snapshot.last_updated, "normalized");
        assert_eq!(snapshot.workspaces.len(), 1);
        assert_eq!(snapshot.workspaces[0].name, "Normalized");

        let _ = std::fs::remove_dir_all(dir);
    }
}
