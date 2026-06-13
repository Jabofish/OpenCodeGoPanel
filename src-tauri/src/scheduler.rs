use crate::auth::AuthStore;
use crate::cache::AppCache;
use crate::client::OpenCodeClient;
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::Duration;

pub struct RefreshScheduler {
    client: Arc<OpenCodeClient>,
    cache: Arc<AppCache>,
    auth_store: Arc<AuthStore>,
    is_visible: Arc<AtomicBool>,
}

impl RefreshScheduler {
    pub fn new(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        auth_store: Arc<AuthStore>,
        is_visible: Arc<AtomicBool>,
    ) -> Self {
        Self {
            client,
            cache,
            auth_store,
            is_visible,
        }
    }

    /// Start adaptive refresh loop: 30s when visible, 10min when hidden
    pub async fn start_adaptive(&self) {
        loop {
            if self.is_visible.load(Ordering::Relaxed) {
                self.do_refresh().await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            } else {
                // Refresh once when hiding, then go to 10min interval
                self.do_refresh().await;
                tokio::time::sleep(Duration::from_secs(600)).await;
            }
        }
    }

    async fn do_refresh(&self) {
        println!("[Scheduler] do_refresh started");
        let stored = match self.auth_store.load_cookies() {
            Some(s) => s,
            None => {
                println!("[Scheduler] No cookies found, setting error");
                self.cache.set_error("Not logged in".into());
                return;
            }
        };

        println!(
            "[Scheduler] Loaded cookies for workspace: {}",
            stored.workspace_id
        );

        let workspace_id = stored.workspace_id.clone();
        let cookies = stored.cookies.clone();

        println!("[Scheduler] Fetching usage, model calls, and full usage history...");
        let usage = self.client.fetch_usage(&cookies, &workspace_id).await;
        let model_calls_with_records = self.client.fetch_all_model_calls(&cookies, &workspace_id, 10).await;
        let monthly_costs = self.client.fetch_monthly_costs(&cookies, &workspace_id).await;

        let mut snapshot = self.cache.get();
        snapshot.workspace_id = workspace_id;

        match usage {
            Ok(u) => {
                println!(
                    "[Scheduler] Usage OK: rolling={}%, weekly={}%, monthly={}%",
                    u.rolling.usage_percent, u.weekly.usage_percent, u.monthly.usage_percent
                );
                snapshot.usage = u;
                snapshot.error = None;
            }
            Err(e) if e == "AUTH_EXPIRED" || e == "REDIRECT_TO_LOGIN" => {
                println!("[Scheduler] Auth expired, clearing cookies");
                self.auth_store.clear_cookies().ok();
                snapshot.error = Some("Session expired. Please log in again.".into());
            }
            Err(e) => {
                println!("[Scheduler] Usage fetch error: {}", e);
                snapshot.error = Some(e);
            }
        }

        match model_calls_with_records {
            Ok((records, m)) => {
                println!(
                    "[Scheduler] Model calls OK: {} records across {} models",
                    records.len(),
                    m.models.len()
                );
                snapshot.model_calls = m;
                snapshot.usage_records = records;
                if snapshot.error.is_none() {
                    snapshot.error = None;
                }
            }
            Err(e) if e == "AUTH_EXPIRED" => {
                println!("[Scheduler] Auth expired (model calls), clearing cookies");
                self.auth_store.clear_cookies().ok();
                snapshot.error = Some("Session expired. Please log in again.".into());
            }
            Err(e) => {
                println!("[Scheduler] Model calls fetch error: {}", e);
                if snapshot.error.is_none() {
                    snapshot.error = Some(e);
                }
            }
        }

        match monthly_costs {
            Ok(c) => {
                println!("[Scheduler] Monthly costs OK: {} entries", c.len());
                snapshot.daily_costs = c;
            }
            Err(e) => {
                println!("[Scheduler] Monthly costs fetch error: {}", e);
                // Don't overwrite error if other fetches succeeded
            }
        }

        snapshot.last_updated = Utc::now().to_rfc3339();
        println!("[Scheduler] do_refresh complete, updating cache");
        self.cache.update(snapshot);
    }

    /// Trigger immediate refresh (called from command handler).
    pub async fn refresh_now(&self) {
        self.do_refresh().await;
    }

    /// Notify scheduler that window visibility changed.
    pub fn set_visible(&self, visible: bool) {
        self.is_visible.store(visible, Ordering::Relaxed);
    }
}
