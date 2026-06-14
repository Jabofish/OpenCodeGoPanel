use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const AUTH_FILE: &str = "opencode-auth.json";

/// Legacy single-workspace format (for migration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCookies {
    pub cookies: Vec<CookieEntry>,
    pub workspace_id: String,
    pub saved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// Multi-workspace auth format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuth {
    pub workspaces: Vec<WorkspaceCredentials>,
    pub active_workspace: String,
    pub saved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCredentials {
    pub workspace_id: String,
    pub cookies: Vec<CookieEntry>,
    pub display_name: String,
    pub added_at: String,
}

/// Workspace info for frontend (without cookies)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub display_name: String,
    pub is_active: bool,
}

pub struct AuthStore {
    data_dir: PathBuf,
}

impl AuthStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn auth_path(&self) -> PathBuf {
        self.data_dir.join(AUTH_FILE)
    }

    /// Load full multi-workspace auth. Auto-migrates from legacy format.
    pub fn load_auth(&self) -> Option<StoredAuth> {
        let path = self.auth_path();
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;

        // Try new format first
        if let Ok(auth) = serde_json::from_str::<StoredAuth>(&content) {
            return Some(auth);
        }

        // Try legacy format and migrate
        if let Ok(old) = serde_json::from_str::<StoredCookies>(&content) {
            println!("[Auth] Migrating from legacy single-workspace format");
            let auth = StoredAuth {
                active_workspace: old.workspace_id.clone(),
                workspaces: vec![WorkspaceCredentials {
                    workspace_id: old.workspace_id,
                    cookies: old.cookies,
                    display_name: String::new(),
                    added_at: old.saved_at.clone(),
                }],
                saved_at: old.saved_at,
            };
            // Persist migrated format
            if let Ok(json) = serde_json::to_string_pretty(&auth) {
                let _ = std::fs::write(&path, json);
            }
            return Some(auth);
        }

        None
    }

    /// Save full auth to disk.
    fn save_auth(&self, auth: &StoredAuth) -> Result<(), String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(auth).map_err(|e| e.to_string())?;
        std::fs::write(self.auth_path(), content).map_err(|e| e.to_string())
    }

    /// Add or update a workspace. Automatically sets it as active.
    pub fn add_workspace(
        &self,
        cookies: Vec<CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        let mut auth = self.load_auth().unwrap_or(StoredAuth {
            workspaces: Vec::new(),
            active_workspace: workspace_id.clone(),
            saved_at: String::new(),
        });

        let now = chrono::Utc::now().to_rfc3339();
        let display_name = if workspace_id.len() > 8 {
            format!("{}…", &workspace_id[..8])
        } else {
            workspace_id.clone()
        };

        // Update existing or add new
        if let Some(ws) = auth
            .workspaces
            .iter_mut()
            .find(|w| w.workspace_id == workspace_id)
        {
            ws.cookies = cookies;
        } else {
            auth.workspaces.push(WorkspaceCredentials {
                workspace_id: workspace_id.clone(),
                cookies,
                display_name,
                added_at: now.clone(),
            });
        }

        auth.active_workspace = workspace_id;
        auth.saved_at = now;
        self.save_auth(&auth)
    }

    /// Switch active workspace.
    pub fn switch_workspace(&self, workspace_id: &str) -> Result<(), String> {
        let mut auth = self.load_auth().ok_or("No auth data found")?;
        if !auth.workspaces.iter().any(|w| w.workspace_id == workspace_id) {
            return Err("Workspace not found".into());
        }
        auth.active_workspace = workspace_id.to_string();
        auth.saved_at = chrono::Utc::now().to_rfc3339();
        self.save_auth(&auth)
    }

    /// Get cookies for the active workspace (backward-compatible).
    pub fn load_cookies(&self) -> Option<StoredCookies> {
        let auth = self.load_auth()?;
        let ws = auth
            .workspaces
            .iter()
            .find(|w| w.workspace_id == auth.active_workspace)?;
        Some(StoredCookies {
            cookies: ws.cookies.clone(),
            workspace_id: ws.workspace_id.clone(),
            saved_at: auth.saved_at.clone(),
        })
    }

    /// List all workspaces for frontend display.
    pub fn list_workspaces(&self) -> Vec<WorkspaceInfo> {
        match self.load_auth() {
            Some(auth) => auth
                .workspaces
                .iter()
                .map(|w| WorkspaceInfo {
                    workspace_id: w.workspace_id.clone(),
                    display_name: if w.display_name.is_empty() {
                        w.workspace_id.clone()
                    } else {
                        w.display_name.clone()
                    },
                    is_active: w.workspace_id == auth.active_workspace,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Legacy save_cookies (for backward compat with existing callers)
    pub fn save_cookies(
        &self,
        cookies: Vec<CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        self.add_workspace(cookies, workspace_id)
    }

    /// Check if stored cookies exist.
    pub fn has_valid_cookies(&self) -> bool {
        self.load_cookies().is_some()
    }

    /// Delete all stored auth data.
    pub fn clear_cookies(&self) -> Result<(), String> {
        let path = self.auth_path();
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
