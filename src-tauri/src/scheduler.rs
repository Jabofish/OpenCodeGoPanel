use crate::auth::AuthStore;
use crate::cache::AppCache;
use crate::client::OpenCodeClient;
use crate::models::{AppDataSnapshot, UsageRecord};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::Duration;

const USAGE_PAGE_SIZE: usize = 50;
const MAX_USAGE_PAGES: u32 = 10_000;
const USAGE_UPDATE_EVERY_PAGES: u32 = 5;

pub struct RefreshScheduler {
    client: Arc<OpenCodeClient>,
    cache: Arc<AppCache>,
    auth_store: Arc<AuthStore>,
    is_visible: Arc<AtomicBool>,
    is_refreshing: Arc<AtomicBool>,
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
            is_refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start adaptive refresh loop: 30s when visible, 10min when hidden
    pub async fn start_adaptive(&self) {
        loop {
            if self.is_visible.load(Ordering::Relaxed) {
                self.refresh_now().await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            } else {
                // Refresh once when hiding, then go to 10min interval
                self.refresh_now().await;
                tokio::time::sleep(Duration::from_secs(600)).await;
            }
        }
    }

    async fn do_refresh(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        auth_store: Arc<AuthStore>,
        is_refreshing: Arc<AtomicBool>,
    ) {
        println!("[Scheduler] do_refresh started");
        let stored = match auth_store.load_cookies() {
            Some(s) => s,
            None => {
                println!("[Scheduler] No cookies found, setting error");
                cache.set_error("Not logged in".into());
                is_refreshing.store(false, Ordering::Release);
                return;
            }
        };

        println!(
            "[Scheduler] Loaded cookies for workspace: {}",
            stored.workspace_id
        );

        let workspace_id = stored.workspace_id.clone();
        let cookies = stored.cookies.clone();

        cache.update_with(|snapshot| {
            Self::prepare_workspace(snapshot, &workspace_id);
            snapshot.last_updated = Utc::now().to_rfc3339();
        });

        println!("[Scheduler] Fetching basic usage first...");
        match client.fetch_usage(&cookies, &workspace_id).await {
            Ok(u) => {
                println!(
                    "[Scheduler] Usage OK: rolling={}%, weekly={}%, monthly={}%",
                    u.rolling.usage_percent, u.weekly.usage_percent, u.monthly.usage_percent
                );
                cache.update_with(|snapshot| {
                    Self::prepare_workspace(snapshot, &workspace_id);
                    snapshot.usage = u;
                    snapshot.error = None;
                    snapshot.last_updated = Utc::now().to_rfc3339();
                });
            }
            Err(e) if e == "AUTH_EXPIRED" || e == "REDIRECT_TO_LOGIN" => {
                println!("[Scheduler] Auth expired, clearing cookies");
                auth_store.clear_cookies().ok();
                cache.set_error("Session expired. Please log in again.".into());
                is_refreshing.store(false, Ordering::Release);
                return;
            }
            Err(e) => {
                println!("[Scheduler] Usage fetch error: {}", e);
                cache.set_error(e);
                is_refreshing.store(false, Ordering::Release);
                return;
            }
        }

        println!("[Scheduler] Basic usage cached; continuing slow data refresh in background");
        let records_client = client.clone();
        let records_cache = cache.clone();
        let records_workspace_id = workspace_id.clone();
        let records_cookies = cookies.clone();

        let costs_client = client.clone();
        let costs_cache = cache.clone();
        let costs_workspace_id = workspace_id.clone();
        let costs_cookies = cookies.clone();
        let costs_auth = auth_store.clone();

        tokio::spawn(async move {
            let records = Self::refresh_usage_records_incremental(
                records_client,
                records_cache.clone(),
                records_cookies,
                records_workspace_id.clone(),
            )
            .await;

            if let Err(e) = records {
                Self::handle_fetch_error(&records_cache, &auth_store, e, "usage records");
            }

            let costs = Self::refresh_monthly_costs(
                costs_client,
                costs_cache.clone(),
                costs_cookies,
                costs_workspace_id,
            )
            .await;

            if let Err(e) = costs {
                Self::handle_fetch_error(&costs_cache, &costs_auth, e, "monthly costs");
            }

            println!("[Scheduler] Slow data refresh complete");
            is_refreshing.store(false, Ordering::Release);
        });
    }

    async fn refresh_usage_records_incremental(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        cookies: Vec<crate::auth::CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        let cached = cache.get();
        let had_cached_records =
            cached.workspace_id == workspace_id && !cached.usage_records.is_empty();
        let mut known_ids: HashSet<String> = if cached.workspace_id == workspace_id {
            cached.usage_records.iter().map(|r| r.id.clone()).collect()
        } else {
            HashSet::new()
        };

        let mut pending_records = Vec::new();
        let mut total_fetched = 0usize;
        let mut total_new = 0usize;

        for page in 0..MAX_USAGE_PAGES {
            let page_records = match client.fetch_usage_page(&cookies, &workspace_id, page).await {
                Ok(records) => records,
                Err(e) => {
                    Self::apply_usage_records(
                        &cache,
                        &workspace_id,
                        std::mem::take(&mut pending_records),
                    );
                    return Err(e);
                }
            };

            if page_records.is_empty() {
                break;
            }

            let fetched = page_records.len();
            let new_in_page = page_records
                .iter()
                .filter(|record| !known_ids.contains(&record.id))
                .count();

            for record in &page_records {
                known_ids.insert(record.id.clone());
            }

            total_fetched += fetched;
            total_new += new_in_page;
            pending_records.extend(page_records);

            let reached_known_tail = had_cached_records && new_in_page == 0;
            let reached_last_page = fetched < USAGE_PAGE_SIZE || reached_known_tail;
            let should_flush = page == 0
                || page % USAGE_UPDATE_EVERY_PAGES == USAGE_UPDATE_EVERY_PAGES - 1
                || reached_last_page;

            if should_flush {
                Self::apply_usage_records(
                    &cache,
                    &workspace_id,
                    std::mem::take(&mut pending_records),
                );
            }

            if reached_last_page {
                break;
            }
        }

        if !pending_records.is_empty() {
            Self::apply_usage_records(&cache, &workspace_id, pending_records);
        }

        println!(
            "[Scheduler] Usage records refresh OK: fetched {} records, {} new",
            total_fetched, total_new
        );
        Ok(())
    }

    async fn refresh_monthly_costs(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        cookies: Vec<crate::auth::CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        let costs = client.fetch_monthly_costs(&cookies, &workspace_id).await?;
        println!("[Scheduler] Monthly costs OK: {} entries", costs.len());
        cache.update_with(|snapshot| {
            Self::prepare_workspace(snapshot, &workspace_id);
            snapshot.daily_costs = costs;
            snapshot.last_updated = Utc::now().to_rfc3339();
        });
        Ok(())
    }

    fn apply_usage_records(cache: &AppCache, workspace_id: &str, incoming: Vec<UsageRecord>) {
        if incoming.is_empty() {
            return;
        }

        cache.update_with(|snapshot| {
            Self::prepare_workspace(snapshot, workspace_id);
            Self::merge_usage_records(&mut snapshot.usage_records, incoming);
            snapshot.model_calls = OpenCodeClient::agg_stats_from_records(&snapshot.usage_records);
            snapshot.error = None;
            snapshot.last_updated = Utc::now().to_rfc3339();
        });
    }

    fn merge_usage_records(existing: &mut Vec<UsageRecord>, incoming: Vec<UsageRecord>) {
        let mut positions: HashMap<String, usize> = existing
            .iter()
            .enumerate()
            .map(|(idx, record)| (record.id.clone(), idx))
            .collect();

        for record in incoming {
            if let Some(idx) = positions.get(&record.id).copied() {
                existing[idx] = record;
            } else {
                positions.insert(record.id.clone(), existing.len());
                existing.push(record);
            }
        }

        existing.sort_by(|a, b| {
            b.time_created
                .cmp(&a.time_created)
                .then_with(|| b.id.cmp(&a.id))
        });
    }

    fn prepare_workspace(snapshot: &mut AppDataSnapshot, workspace_id: &str) {
        if snapshot.workspace_id != workspace_id {
            snapshot.workspace_id = workspace_id.to_string();
            snapshot.model_calls.models.clear();
            snapshot.model_calls.total_calls = 0;
            snapshot.usage_records.clear();
            snapshot.daily_costs.clear();
        } else if snapshot.workspace_id.is_empty() {
            snapshot.workspace_id = workspace_id.to_string();
        }
    }

    fn handle_fetch_error(cache: &AppCache, auth_store: &AuthStore, error: String, label: &str) {
        if error == "AUTH_EXPIRED" || error == "REDIRECT_TO_LOGIN" {
            println!("[Scheduler] Auth expired ({}), clearing cookies", label);
            auth_store.clear_cookies().ok();
            cache.set_error("Session expired. Please log in again.".into());
        } else {
            println!("[Scheduler] {} fetch error: {}", label, error);
            cache.update_with(|snapshot| {
                if snapshot.error.is_none() {
                    snapshot.error = Some(error);
                }
            });
        }
    }

    /// Trigger immediate refresh (called from command handler).
    pub async fn refresh_now(&self) {
        if self
            .is_refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            println!("[Scheduler] Refresh already running; skipping");
            return;
        }

        Self::do_refresh(
            self.client.clone(),
            self.cache.clone(),
            self.auth_store.clone(),
            self.is_refreshing.clone(),
        )
        .await;
    }

    /// Notify scheduler that window visibility changed.
    pub fn set_visible(&self, visible: bool) {
        self.is_visible.store(visible, Ordering::Relaxed);
    }
}
