use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const AUTH_FILE: &str = "opencode-auth.json";

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

    /// Load cookies from disk. Returns None if file missing or corrupt.
    pub fn load_cookies(&self) -> Option<StoredCookies> {
        let path = self.auth_path();
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str::<StoredCookies>(&content).ok()
    }

    /// Save cookies to disk.
    pub fn save_cookies(
        &self,
        cookies: Vec<CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let stored = StoredCookies {
            cookies,
            workspace_id,
            saved_at: chrono::Utc::now().to_rfc3339(),
        };
        let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
        std::fs::write(self.auth_path(), content).map_err(|e| e.to_string())
    }

    /// Check if stored cookies exist.
    pub fn has_valid_cookies(&self) -> bool {
        self.load_cookies().is_some()
    }

    /// Delete stored cookies.
    pub fn clear_cookies(&self) -> Result<(), String> {
        let path = self.auth_path();
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
