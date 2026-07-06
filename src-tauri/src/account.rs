use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const ACCOUNTS_DIR: &str = "accounts";
pub const ACCOUNT_FILE: &str = "opencode-account.json";

/// Metadata for one account, stored in the global settings index.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountInfo {
    pub id: String,
    pub display_name: String,
    pub added_at: String,
    pub last_used_at: String,
}

/// Per-account preferences, stored in `accounts/<id>/opencode-account.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountSettings {
    pub monthly_budget: u32,
    pub usage_threshold: u32,
    pub mini_badge_source: String,
    pub workspace_profiles: HashMap<String, crate::settings_store::WorkspaceProfile>,
    pub recent_workspaces: Vec<String>,
}

impl AccountSettings {
    pub fn defaults() -> Self {
        // Mirror the current AppSettings defaults for the 5 moved fields.
        Self {
            monthly_budget: 6000,
            usage_threshold: 80,
            mini_badge_source: "auto".into(),
            workspace_profiles: HashMap::new(),
            recent_workspaces: Vec::new(),
        }
    }
}

/// Resolve the per-account data directory: `<data_dir>/accounts/<id>`.
pub fn account_dir(data_dir: &Path, account_id: &str) -> PathBuf {
    data_dir.join(ACCOUNTS_DIR).join(account_id)
}

static ACCOUNT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique-enough account ID without pulling in a UUID crate.
/// Combines wall-clock nanos + a per-process counter → 16 hex chars.
pub fn new_account_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = ACCOUNT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mixed = nanos ^ (counter.wrapping_mul(0x9E3779B97F4A7C15));
    format!("acc-{:016x}", mixed)
}

/// Default display name for the Nth account (1-indexed).
pub fn default_display_name(existing_count: usize) -> String {
    format!("Account {}", existing_count + 1)
}

/// Pure owner of the account index. No I/O — callers persist the result.
pub struct AccountsManager {
    accounts: Vec<AccountInfo>,
    active_account_id: String,
}

impl AccountsManager {
    pub fn new(accounts: Vec<AccountInfo>, active_account_id: String) -> Self {
        Self {
            accounts,
            active_account_id,
        }
    }

    pub fn list(&self) -> &[AccountInfo] {
        &self.accounts
    }

    pub fn active(&self) -> Option<&AccountInfo> {
        self.accounts
            .iter()
            .find(|a| a.id == self.active_account_id)
    }

    pub fn active_account_id(&self) -> &str {
        &self.active_account_id
    }

    /// Add a new account, set it active, return the new entry (borrowed).
    pub fn add(&mut self, display_name: Option<String>) -> &AccountInfo {
        let id = new_account_id();
        let now = chrono::Utc::now().to_rfc3339();
        let name = display_name.unwrap_or_else(|| default_display_name(self.accounts.len()));
        let info = AccountInfo {
            id: id.clone(),
            display_name: name,
            added_at: now.clone(),
            last_used_at: now,
        };
        self.accounts.push(info);
        self.active_account_id = id;
        self.accounts.last().unwrap()
    }

    pub fn rename(&mut self, id: &str, new_name: String) -> Result<(), String> {
        let info = self
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("Account {} not found", id))?;
        info.display_name = new_name;
        Ok(())
    }

    /// Remove an account. Refuses if it's the only one. If it was active,
    /// moves activation to the first remaining account.
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        if self.accounts.len() <= 1 {
            return Err("Cannot remove the only account".into());
        }
        let pos = self
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| format!("Account {} not found", id))?;
        self.accounts.remove(pos);
        if self.active_account_id == id {
            self.active_account_id = self.accounts.first().unwrap().id.clone();
        }
        Ok(())
    }

    /// Set the active account, updating its last_used_at.
    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        let info = self
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("Account {} not found", id))?;
        info.last_used_at = chrono::Utc::now().to_rfc3339();
        self.active_account_id = id.to_string();
        Ok(())
    }

    pub fn into_parts(self) -> (Vec<AccountInfo>, String) {
        (self.accounts, self.active_account_id)
    }
}

use crate::settings_store::SETTINGS_FILE;

/// One-time migration: move the legacy single-account data files into
/// `accounts/<default_id>/` and seed the account index in the global settings.
///
/// Rules:
/// - If `accounts/` already exists → already migrated (leave alone, even if a
///   stale top-level `opencode-auth.json` is present).
/// - If `accounts/` doesn't exist but a top-level `opencode-auth.json` exists
///   → migrate into one default account.
/// - Otherwise (fresh install) → no-op.
pub fn migrate_to_accounts_layout(data_dir: &Path) -> Result<(), String> {
    let accounts_root = data_dir.join(ACCOUNTS_DIR);
    if accounts_root.exists() {
        return Ok(());
    }
    let legacy_auth = data_dir.join("opencode-auth.json");
    if !legacy_auth.exists() {
        return Ok(()); // fresh install
    }

    // Create the default account.
    let default_id = new_account_id();
    let acct_dir = accounts_root.join(&default_id);
    std::fs::create_dir_all(&acct_dir)
        .map_err(|e| format!("create account dir {}: {}", acct_dir.display(), e))?;

    // Move the three data files.
    for name in [
        "opencode-auth.json",
        "opencode-history.json",
        "opencode-cache.json",
    ] {
        let from = data_dir.join(name);
        let to = acct_dir.join(name);
        if from.exists() {
            std::fs::rename(&from, &to)
                .or_else(|_| {
                    // rename can fail across volumes; fall back to copy+delete
                    std::fs::copy(&from, &to)?;
                    std::fs::remove_file(&from)
                })
                .map_err(|e| format!("move {}: {}", name, e))?;
        }
    }

    // Read the legacy settings (still has the 5 fields), write them to the account file.
    let settings_path = data_dir.join(SETTINGS_FILE);
    let legacy_settings: crate::settings_store::AppSettings =
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();

    let acct_settings = AccountSettings {
        monthly_budget: legacy_settings.monthly_budget,
        usage_threshold: legacy_settings.usage_threshold,
        mini_badge_source: legacy_settings.mini_badge_source.clone(),
        workspace_profiles: legacy_settings.workspace_profiles.clone(),
        recent_workspaces: legacy_settings.recent_workspaces.clone(),
    };
    let acct_json = serde_json::to_string_pretty(&acct_settings)
        .map_err(|e| format!("serialize account settings: {}", e))?;
    std::fs::write(acct_dir.join(ACCOUNT_FILE), acct_json)
        .map_err(|e| format!("write account file: {}", e))?;

    // Update the global settings with the account index + active_account_id.
    let mut global = legacy_settings;
    global.active_account_id = default_id.clone();
    let now = chrono::Utc::now().to_rfc3339();
    global.accounts = vec![AccountInfo {
        id: default_id.clone(),
        display_name: default_display_name(0), // "Account 1"
        added_at: now.clone(),
        last_used_at: now,
    }];
    let global_json = serde_json::to_string_pretty(&global.normalize())
        .map_err(|e| format!("serialize global settings: {}", e))?;
    std::fs::write(&settings_path, global_json)
        .map_err(|e| format!("write global settings: {}", e))?;

    println!(
        "[Account] Migrated legacy single-account data to {}",
        acct_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_store::{AppSettings, WorkspaceProfile};

    fn temp_dir() -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ocp-acct-{}-{}", pid, nanos))
    }

    #[test]
    fn account_info_defaults_are_empty_strings() {
        let info = AccountInfo::default();
        assert!(info.id.is_empty());
        assert!(info.display_name.is_empty());
        assert!(info.added_at.is_empty());
        assert!(info.last_used_at.is_empty());
    }

    #[test]
    fn account_settings_defaults_match_legacy_globals() {
        let s = AccountSettings::defaults();
        assert_eq!(s.monthly_budget, 6000);
        assert_eq!(s.usage_threshold, 80);
        assert_eq!(s.mini_badge_source, "auto");
        assert!(s.workspace_profiles.is_empty());
        assert!(s.recent_workspaces.is_empty());
    }

    #[test]
    fn account_settings_round_trips_through_json() {
        let mut s = AccountSettings::defaults();
        s.monthly_budget = 9999;
        s.usage_threshold = 75;
        let json = serde_json::to_string(&s).unwrap();
        // camelCase keys
        assert!(json.contains("\"monthlyBudget\":9999"));
        assert!(json.contains("\"usageThreshold\":75"));
        let back: AccountSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.monthly_budget, 9999);
        assert_eq!(back.usage_threshold, 75);
    }

    #[test]
    fn account_settings_defaults_when_empty_json() {
        let back: AccountSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(back.monthly_budget, 0); // serde default, NOT defaults()
        assert_eq!(back.mini_badge_source, "");
    }

    #[test]
    fn account_dir_resolves_under_accounts_subdir() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let resolved = account_dir(&dir, "acc-123");
        assert_eq!(resolved, dir.join("accounts").join("acc-123"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_account_id_is_nonempty_and_unique() {
        let a = super::new_account_id();
        let b = super::new_account_id();
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b, "consecutive IDs must differ");
    }

    #[test]
    fn default_display_name_increments() {
        assert_eq!(super::default_display_name(0), "Account 1");
        assert_eq!(super::default_display_name(2), "Account 3");
    }

    #[test]
    fn manager_add_generates_id_sets_active_and_metadata() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let added = m.add(None).clone();
        assert!(!added.id.is_empty());
        assert_eq!(added.display_name, "Account 1");
        assert!(!added.added_at.is_empty());
        assert_eq!(added.last_used_at, added.added_at);
        assert_eq!(m.active_account_id(), added.id);
        assert_eq!(m.list().len(), 1);
    }

    #[test]
    fn manager_add_with_custom_display_name() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let added = m.add(Some("Work".into()));
        assert_eq!(added.display_name, "Work");
    }

    #[test]
    fn manager_set_active_updates_last_used_at() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let a = m.add(None).clone();
        let _b = m.add(None).clone();
        assert_eq!(m.active_account_id(), _b.id);
        m.set_active(&a.id).unwrap();
        assert_eq!(m.active_account_id(), a.id);
        let active = m.active().unwrap();
        assert_eq!(active.id, a.id);
        assert!(!active.last_used_at.is_empty());
    }

    #[test]
    fn manager_set_active_unknown_id_errors() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        m.add(None);
        assert!(m.set_active("does-not-exist").is_err());
    }

    #[test]
    fn manager_rename_updates_display_name() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let a = m.add(None).clone();
        m.rename(&a.id, "Personal".into()).unwrap();
        assert_eq!(
            m.list().iter().find(|x| x.id == a.id).unwrap().display_name,
            "Personal"
        );
    }

    #[test]
    fn manager_remove_deletes_entry() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let a = m.add(None).clone();
        let b = m.add(None).clone();
        m.remove(&a.id).unwrap();
        assert_eq!(m.list().len(), 1);
        assert!(m.list().iter().all(|x| x.id != a.id));
        assert_eq!(m.active_account_id(), b.id);
    }

    #[test]
    fn manager_remove_refuses_last_account() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        let a = m.add(None).clone();
        assert!(m.remove(&a.id).is_err());
        assert_eq!(m.list().len(), 1);
    }

    #[test]
    fn manager_remove_unknown_id_errors() {
        let mut m = super::AccountsManager::new(Vec::new(), String::new());
        m.add(None);
        assert!(m.remove("nope").is_err());
    }

    fn legacy_auth_json(workspace_id: &str) -> String {
        serde_json::json!({
            "active_workspace": workspace_id,
            "workspaces": [{
                "workspace_id": workspace_id,
                "cookies": [],
                "display_name": "",
                "added_at": "2026-01-01T00:00:00Z"
            }],
            "saved_at": "2026-01-01T00:00:00Z"
        })
        .to_string()
    }

    #[test]
    fn migrates_legacy_single_account() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opencode-auth.json"), legacy_auth_json("ws-1")).unwrap();
        std::fs::write(dir.join("opencode-history.json"), "[]").unwrap();
        std::fs::write(
            dir.join(crate::cache::CACHE_FILE),
            serde_json::json!({
                "version": 2,
                "active_workspace": "ws-1",
                "workspaces": [],
                "snapshots": {}
            })
            .to_string(),
        )
        .unwrap();

        let mut workspace_profiles = std::collections::HashMap::new();
        workspace_profiles.insert(
            "ws-1".to_string(),
            WorkspaceProfile {
                alias: "Main".into(),
                favorite: true,
                mini_badge_source: "auto".into(),
            },
        );
        let legacy = AppSettings {
            monthly_budget: 6000,
            usage_threshold: 80,
            workspace_profiles,
            recent_workspaces: vec!["ws-1".into()],
            ..AppSettings::default()
        };
        std::fs::write(
            dir.join(crate::settings_store::SETTINGS_FILE),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        super::migrate_to_accounts_layout(&dir).unwrap();

        let accounts_dir = dir.join(ACCOUNTS_DIR);
        assert!(accounts_dir.exists());
        let entries: Vec<_> = std::fs::read_dir(&accounts_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let account_dir = entries[0].as_ref().unwrap().path();
        assert!(account_dir.join("opencode-auth.json").exists());
        assert!(account_dir.join("opencode-history.json").exists());
        assert!(account_dir.join(crate::cache::CACHE_FILE).exists());
        assert!(account_dir.join(ACCOUNT_FILE).exists());
        assert!(!dir.join("opencode-auth.json").exists());

        let global_str =
            std::fs::read_to_string(dir.join(crate::settings_store::SETTINGS_FILE)).unwrap();
        let global: AppSettings = serde_json::from_str(&global_str).unwrap();
        assert_eq!(global.accounts.len(), 1);
        assert!(!global.active_account_id.is_empty());

        let account_str = std::fs::read_to_string(account_dir.join(ACCOUNT_FILE)).unwrap();
        let account: AccountSettings = serde_json::from_str(&account_str).unwrap();
        assert_eq!(account.monthly_budget, 6000);
        assert_eq!(account.usage_threshold, 80);
        assert_eq!(account.workspace_profiles.len(), 1);
        assert_eq!(account.recent_workspaces, vec!["ws-1".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_idempotent() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opencode-auth.json"), legacy_auth_json("ws-1")).unwrap();
        std::fs::write(
            dir.join(crate::settings_store::SETTINGS_FILE),
            serde_json::to_string_pretty(&AppSettings::default()).unwrap(),
        )
        .unwrap();

        super::migrate_to_accounts_layout(&dir).unwrap();
        let first_entries: Vec<_> = std::fs::read_dir(dir.join(ACCOUNTS_DIR)).unwrap().collect();
        super::migrate_to_accounts_layout(&dir).unwrap();
        let second_entries: Vec<_> = std::fs::read_dir(dir.join(ACCOUNTS_DIR)).unwrap().collect();
        assert_eq!(first_entries.len(), second_entries.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_no_legacy_files_does_nothing() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        super::migrate_to_accounts_layout(&dir).unwrap();
        assert!(!dir.join(ACCOUNTS_DIR).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_partial_state_leaves_accounts_dir_alone() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join(ACCOUNTS_DIR).join("existing-acc");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(dir.join("opencode-auth.json"), legacy_auth_json("ws-1")).unwrap();

        super::migrate_to_accounts_layout(&dir).unwrap();
        assert!(existing.exists());
        assert!(!dir.join(ACCOUNTS_DIR).join("opencode-auth.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
