use crate::models::{AppDataSnapshot, HistoryEntry};
use chrono::{Duration, Utc};
use std::path::PathBuf;
use std::sync::RwLock;

const HISTORY_FILE: &str = "opencode-history.json";
const DEFAULT_KEEP_DAYS: u32 = 90;

pub struct HistoryStore {
    data: RwLock<Vec<HistoryEntry>>,
    history_path: PathBuf,
}

impl HistoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let history_path = data_dir.join(HISTORY_FILE);
        let data = std::fs::read_to_string(&history_path)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<HistoryEntry>>(&content).ok())
            .unwrap_or_default();

        Self {
            data: RwLock::new(data),
            history_path,
        }
    }

    /// Record today's snapshot into history. Updates existing entry for today or appends new.
    pub fn record(&self, snapshot: &AppDataSnapshot) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let total_cost: i64 = snapshot.daily_costs.iter().map(|c| c.total_cost).sum();

        let entry = HistoryEntry {
            date: today.clone(),
            rolling_pct: snapshot.usage.rolling.usage_percent,
            weekly_pct: snapshot.usage.weekly.usage_percent,
            monthly_pct: snapshot.usage.monthly.usage_percent,
            total_cost,
            recorded_at: Utc::now().to_rfc3339(),
        };

        if let Ok(mut writer) = self.data.write() {
            // Update existing entry for today or push new
            if let Some(existing) = writer.iter_mut().find(|e| e.date == today) {
                *existing = entry;
            } else {
                writer.push(entry);
            }

            // Prune old entries
            let cutoff = (Utc::now() - Duration::days(DEFAULT_KEEP_DAYS as i64))
                .format("%Y-%m-%d")
                .to_string();
            writer.retain(|e| e.date >= cutoff);

            // Sort by date ascending
            writer.sort_by(|a, b| a.date.cmp(&b.date));

            self.persist_locked(&writer);
        }
    }

    /// Get history entries for the last N days.
    pub fn get_entries(&self, days: u32) -> Vec<HistoryEntry> {
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();

        self.data
            .read()
            .map(|reader| {
                reader
                    .iter()
                    .filter(|e| e.date >= cutoff)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn persist_locked(&self, entries: &[HistoryEntry]) {
        if let Some(parent) = self.history_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("[History] Failed to create history dir: {}", e);
                return;
            }
        }

        match serde_json::to_string(entries) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&self.history_path, content) {
                    eprintln!("[History] Failed to write history: {}", e);
                }
            }
            Err(e) => eprintln!("[History] Failed to serialize history: {}", e),
        }
    }
}
