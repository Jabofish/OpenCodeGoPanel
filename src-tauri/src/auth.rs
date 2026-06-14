use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const AUTH_FILE: &str = "opencode-auth.json";
const AUTH_ENCRYPTED_PREFIX: &str = "dpapi:v1:";

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
        let raw_content = std::fs::read_to_string(&path).ok()?;
        let (content, needs_rewrite) = match Self::decode_auth_content(&raw_content) {
            Ok(decoded) => (decoded, !raw_content.starts_with(AUTH_ENCRYPTED_PREFIX)),
            Err(e) => {
                eprintln!("[Auth] Failed to decode auth file: {}", e);
                return None;
            }
        };

        // Try new format first
        if let Ok(auth) = serde_json::from_str::<StoredAuth>(&content) {
            if needs_rewrite {
                let _ = self.save_auth(&auth);
            }
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
            let _ = self.save_auth(&auth);
            return Some(auth);
        }

        None
    }

    /// Save full auth to disk.
    fn save_auth(&self, auth: &StoredAuth) -> Result<(), String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(auth).map_err(|e| e.to_string())?;
        let content = Self::encode_auth_content(&content)?;
        std::fs::write(self.auth_path(), content).map_err(|e| e.to_string())
    }

    fn decode_auth_content(content: &str) -> Result<String, String> {
        if let Some(encoded) = content.strip_prefix(AUTH_ENCRYPTED_PREFIX) {
            let encrypted = BASE64
                .decode(encoded.trim())
                .map_err(|e| format!("Invalid encrypted auth encoding: {}", e))?;
            let decrypted = dpapi_unprotect(&encrypted)?;
            String::from_utf8(decrypted).map_err(|e| format!("Invalid auth UTF-8: {}", e))
        } else {
            Ok(content.to_string())
        }
    }

    fn encode_auth_content(content: &str) -> Result<String, String> {
        let encrypted = dpapi_protect(content.as_bytes())?;
        Ok(format!(
            "{}{}",
            AUTH_ENCRYPTED_PREFIX,
            BASE64.encode(encrypted)
        ))
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
        if !auth
            .workspaces
            .iter()
            .any(|w| w.workspace_id == workspace_id)
        {
            let active = auth
                .workspaces
                .iter()
                .find(|w| w.workspace_id == auth.active_workspace)
                .or_else(|| auth.workspaces.first())
                .cloned()
                .ok_or("No workspace credentials found")?;

            auth.workspaces.push(WorkspaceCredentials {
                workspace_id: workspace_id.to_string(),
                cookies: active.cookies,
                display_name: workspace_id.to_string(),
                added_at: chrono::Utc::now().to_rfc3339(),
            });
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

#[cfg(target_os = "windows")]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data
            .len()
            .try_into()
            .map_err(|_| "Auth data is too large to encrypt".to_string())?,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &input,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err(format!(
            "DPAPI encryption failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(encrypted)
}

#[cfg(target_os = "windows")]
fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data
            .len()
            .try_into()
            .map_err(|_| "Encrypted auth data is too large".to_string())?,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err(format!(
            "DPAPI decryption failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let decrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(decrypted)
}

#[cfg(not(target_os = "windows"))]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

#[cfg(not(target_os = "windows"))]
fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::{AuthStore, AUTH_ENCRYPTED_PREFIX};

    #[test]
    fn auth_content_round_trips_through_encrypted_wrapper() {
        let raw = r#"{"workspaces":[],"active_workspace":"","saved_at":""}"#;

        let encoded = AuthStore::encode_auth_content(raw).unwrap();
        assert!(encoded.starts_with(AUTH_ENCRYPTED_PREFIX));
        assert!(!encoded.contains("workspaces"));

        let decoded = AuthStore::decode_auth_content(&encoded).unwrap();
        assert_eq!(decoded, raw);
    }
}
