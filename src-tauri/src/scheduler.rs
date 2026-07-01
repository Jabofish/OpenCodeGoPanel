use crate::auth::AuthStore;
use crate::cache::AppCache;
use crate::client::OpenCodeClient;
use crate::history::HistoryStore;
use crate::models::{AppDataSnapshot, UsageRecord};
use crate::notification_rules::{self, NotificationRuleState, ThresholdNotificationState};
use crate::settings_store::SettingsStore;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;
use tokio::time::Duration;

const USAGE_PAGE_SIZE: usize = 50;
const MAX_USAGE_PAGES: u32 = 10_000;
const USAGE_UPDATE_EVERY_PAGES: u32 = 5;
const DEFAULT_VISIBLE_INTERVAL_SECS: u64 = 30;
const MIN_VISIBLE_INTERVAL_SECS: u64 = 15;
const MIN_HIDDEN_INTERVAL_SECS: u64 = 60;
const MAX_REFRESH_INTERVAL_SECS: u64 = 3600;

/// Bundled Arcs passed into `do_refresh` to keep the argument count reasonable.
struct RefreshContext {
    client: Arc<OpenCodeClient>,
    cache: Arc<AppCache>,
    auth_store: Arc<AuthStore>,
    history_store: Arc<HistoryStore>,
    settings_store: Arc<SettingsStore>,
    notification_rules: Arc<NotificationRuleState>,
    threshold_notifications: Arc<ThresholdNotificationState>,
    is_refreshing: Arc<AtomicBool>,
    threshold: Arc<AtomicU32>,
    consecutive_failures: Arc<AtomicU32>,
    /// Suppress notifications on first refresh after startup (seed state without alerting)
    skip_notifications: Arc<AtomicBool>,
}

pub struct RefreshScheduler {
    client: Arc<OpenCodeClient>,
    cache: Arc<AppCache>,
    auth_store: Arc<AuthStore>,
    history_store: Arc<HistoryStore>,
    settings_store: Arc<SettingsStore>,
    notification_rules: Arc<NotificationRuleState>,
    threshold_notifications: Arc<ThresholdNotificationState>,
    is_visible: Arc<AtomicBool>,
    is_refreshing: Arc<AtomicBool>,
    /// Usage alert threshold (0 = disabled, 50-95 = percentage)
    threshold: Arc<AtomicU32>,
    /// Whether we've already fired an alert for the current threshold crossing
    alerted: Arc<AtomicBool>,
    /// App handle for sending notifications (set after setup)
    app_handle: Mutex<Option<tauri::AppHandle>>,
    /// Configurable refresh interval when window is visible (seconds)
    visible_interval_secs: Arc<AtomicU64>,
    /// Configurable refresh interval when window is hidden (seconds, 0 = off)
    hidden_interval_secs: Arc<AtomicU64>,
    /// Consecutive refresh failures (reset on success)
    consecutive_failures: Arc<AtomicU32>,
    /// Suppress notifications on first refresh after startup
    skip_notifications: Arc<AtomicBool>,
}

impl RefreshScheduler {
    pub fn new(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        auth_store: Arc<AuthStore>,
        history_store: Arc<HistoryStore>,
        settings_store: Arc<SettingsStore>,
        is_visible: Arc<AtomicBool>,
    ) -> Self {
        let settings = settings_store.get();
        Self {
            client,
            cache,
            auth_store,
            history_store,
            settings_store,
            notification_rules: Arc::new(NotificationRuleState::new()),
            threshold_notifications: Arc::new(ThresholdNotificationState::new()),
            is_visible,
            is_refreshing: Arc::new(AtomicBool::new(false)),
            threshold: Arc::new(AtomicU32::new(0)),
            alerted: Arc::new(AtomicBool::new(false)),
            app_handle: Mutex::new(None),
            visible_interval_secs: Arc::new(AtomicU64::new(normalize_visible_interval(
                settings.refresh_visible_secs,
            ))),
            hidden_interval_secs: Arc::new(AtomicU64::new(normalize_hidden_interval(
                settings.refresh_hidden_secs,
            ))),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            skip_notifications: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Set the AppHandle for notification support (call during setup)
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        if let Ok(mut app_handle) = self.app_handle.lock() {
            *app_handle = Some(handle);
        }
    }

    /// Set the usage alert threshold
    pub fn set_threshold(&self, threshold: u32) {
        self.threshold.store(threshold.min(95), Ordering::Relaxed);
        // Reset alerted flag and threshold notification state when threshold changes
        self.alerted.store(false, Ordering::Relaxed);
        self.threshold_notifications.reset_all();
    }

    /// Get the current usage alert threshold
    pub fn get_threshold(&self) -> u32 {
        self.threshold.load(Ordering::Relaxed)
    }

    /// Start adaptive refresh loop with configurable intervals.
    pub async fn start_adaptive(&self) {
        loop {
            let visible = self.is_visible.load(Ordering::Relaxed);
            let base_secs = if visible {
                self.visible_interval_secs.load(Ordering::Relaxed)
            } else {
                self.hidden_interval_secs.load(Ordering::Relaxed)
            };

            let failures = self.consecutive_failures.load(Ordering::Relaxed);
            let backoff = calculate_backoff(failures);
            let secs = base_secs.saturating_mul(backoff);

            if secs == 0 {
                // Hidden refresh disabled; sleep and re-check visibility
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            if backoff > 1 {
                println!(
                    "[Scheduler] Backoff active: {} consecutive failures, interval {}s (base {}s)",
                    failures, secs, base_secs
                );
            }

            self.refresh_now().await;
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    }

    /// Update configurable refresh intervals at runtime.
    pub fn set_refresh_intervals(&self, visible_secs: u64, hidden_secs: u64) {
        self.visible_interval_secs
            .store(normalize_visible_interval(visible_secs), Ordering::Relaxed);
        self.hidden_interval_secs
            .store(normalize_hidden_interval(hidden_secs), Ordering::Relaxed);
    }

    async fn do_refresh(ctx: RefreshContext, app_handle: Option<tauri::AppHandle>) {
        let stored = match ctx.auth_store.load_cookies() {
            Some(s) => s,
            None => {
                ctx.cache.set_error("Not logged in".into());
                ctx.cache.update_refresh_state(|rs| {
                    rs.is_refreshing = false;
                    rs.phase = "error".into();
                    rs.last_finished_at = Some(Utc::now().to_rfc3339());
                    rs.last_error = Some("Not logged in".into());
                });
                ctx.is_refreshing.store(false, Ordering::Release);
                if let Some(ref handle) = app_handle {
                    let _ = handle.emit(
                        "refresh-complete",
                        serde_json::json!({ "status": "error", "reason": "not_logged_in" }),
                    );
                }
                return;
            }
        };

        let workspace_id = stored.workspace_id.clone();
        let cookies = stored.cookies.clone();

        ctx.cache.update_with(|snapshot| {
            Self::prepare_workspace(snapshot, &workspace_id);
            snapshot.last_updated = Utc::now().to_rfc3339();
        });

        ctx.cache
            .update_refresh_state(|rs| rs.phase = "usage".into());
        match ctx.client.fetch_usage(&cookies, &workspace_id).await {
            Ok((u, workspaces)) => {
                println!(
                    "[Refresh] workspace={} R={}% W={}% M={}%",
                    workspace_id,
                    u.rolling.usage_percent,
                    u.weekly.usage_percent,
                    u.monthly.usage_percent
                );
                ctx.cache.update_with(|snapshot| {
                    if snapshot.workspace_id != workspace_id {
                        println!(
                            "[Scheduler] Ignoring stale usage result for {}",
                            workspace_id
                        );
                        return;
                    }
                    Self::prepare_workspace(snapshot, &workspace_id);
                    snapshot.usage = u.clone();
                    snapshot.workspaces = workspaces;
                    snapshot.error = None;
                    snapshot.last_updated = Utc::now().to_rfc3339();
                });

                // Success: reset consecutive failures
                ctx.consecutive_failures.store(0, Ordering::Relaxed);

                // --- Notification checks ---
                // Skip on first refresh after startup (seed threshold state without alerting)
                let skip = ctx.skip_notifications.load(Ordering::Relaxed);
                let settings = ctx.settings_store.get();
                let thresh = ctx.threshold.load(Ordering::Relaxed);
                let is_quiet = settings.quiet_hours_enabled
                    && notification_rules::is_in_quiet_hours(
                        &settings.quiet_hours_start,
                        &settings.quiet_hours_end,
                    );
                if !skip && !is_quiet {
                    // Quota notification (highest of rolling/weekly/monthly)
                    if settings.notify_quota && thresh >= 50 {
                        // Check each period independently
                        for (period, pct) in [
                            ("Rolling", u.rolling.usage_percent),
                            ("Weekly", u.weekly.usage_percent),
                            ("Monthly", u.monthly.usage_percent),
                        ] {
                            let (should_notify, _) = ctx
                                .threshold_notifications
                                .should_notify_threshold(&workspace_id, period, pct, thresh);
                            if should_notify {
                                if let Some(ref handle) = app_handle {
                                    let _ = handle
                                        .notification()
                                        .builder()
                                        .title("Quota Alert")
                                        .body(format!(
                                            "{} usage reached {}% (threshold {}%).",
                                            period, pct, thresh
                                        ))
                                        .show();
                                    println!(
                                        "[Scheduler] Quota notification: {} at {}%",
                                        period, pct
                                    );
                                }
                            }
                        }
                    }

                    // Refresh failure notification (not applicable on success, but
                    // tracked via consecutive_failures elsewhere)
                    // Budget projection is checked after costs refresh in spawn
                }
            }
            Err(e) if e == "AUTH_EXPIRED" || e == "REDIRECT_TO_LOGIN" => {
                ctx.consecutive_failures.fetch_add(1, Ordering::Relaxed);
                println!(
                    "[Refresh] Auth expired, clearing cookies (consecutive failures: {})",
                    ctx.consecutive_failures.load(Ordering::Relaxed)
                );
                ctx.auth_store.clear_cookies().ok();
                ctx.cache
                    .set_error("Session expired. Please log in again.".into());
                ctx.cache.update_refresh_state(|rs| {
                    rs.is_refreshing = false;
                    rs.phase = "error".into();
                    rs.last_finished_at = Some(Utc::now().to_rfc3339());
                    rs.last_error = Some(e);
                });
                ctx.is_refreshing.store(false, Ordering::Release);
                if let Some(ref handle) = app_handle {
                    let _ = handle.emit(
                        "auth-state-changed",
                        serde_json::json!({ "state": "expired" }),
                    );
                    let _ = handle.emit(
                        "refresh-complete",
                        serde_json::json!({ "status": "error", "reason": "auth_expired" }),
                    );
                }
                return;
            }
            Err(e) => {
                ctx.consecutive_failures.fetch_add(1, Ordering::Relaxed);
                println!(
                    "[Scheduler] Usage fetch error: {} (consecutive failures: {})",
                    e,
                    ctx.consecutive_failures.load(Ordering::Relaxed)
                );
                // Don't clear existing usage data for non-auth errors
                // (workspace may not have a Go plan — let frontend show info message)
                ctx.cache.update_with(|snapshot| {
                    if snapshot.workspace_id != workspace_id {
                        return;
                    }
                    if snapshot.error.is_none() {
                        snapshot.error = Some(e.clone());
                    }
                    snapshot.refresh_state.is_refreshing = false;
                    snapshot.refresh_state.phase = "error".into();
                    snapshot.refresh_state.last_finished_at = Some(Utc::now().to_rfc3339());
                    snapshot.refresh_state.last_error = Some(e.clone());
                    snapshot.last_updated = Utc::now().to_rfc3339();
                });
                ctx.is_refreshing.store(false, Ordering::Release);
                if let Some(ref handle) = app_handle {
                    let _ =
                        handle.emit("refresh-complete", serde_json::json!({ "status": "error" }));
                }
                return;
            }
        }

        ctx.cache
            .update_refresh_state(|rs| rs.phase = "records".into());

        let records_client = ctx.client.clone();
        let records_cache = ctx.cache.clone();
        let records_workspace_id = workspace_id.clone();
        let records_cookies = cookies.clone();

        let costs_client = ctx.client.clone();
        let costs_cache = ctx.cache.clone();
        let costs_workspace_id = workspace_id.clone();
        let costs_cookies = cookies.clone();
        let costs_auth = ctx.auth_store.clone();
        let records_history = ctx.history_store.clone();
        let spawn_notify_rules = ctx.notification_rules.clone();
        let spawn_settings = ctx.settings_store.clone();
        let spawn_app_handle = app_handle.clone();
        let spawn_skip_notifications = ctx.skip_notifications.load(Ordering::Relaxed);

        tokio::spawn(async move {
            let records = Self::refresh_usage_records_incremental(
                records_client,
                records_cache.clone(),
                records_cookies,
                records_workspace_id.clone(),
            )
            .await;

            if let Err(e) = records {
                Self::handle_fetch_error(
                    &records_cache,
                    &costs_auth,
                    e,
                    "usage records",
                    &spawn_app_handle,
                );
            }

            records_cache.update_refresh_state(|rs| rs.phase = "costs".into());

            let costs = Self::refresh_monthly_costs(
                costs_client,
                costs_cache.clone(),
                costs_cookies,
                costs_workspace_id,
            )
            .await;

            if let Err(e) = costs {
                Self::handle_fetch_error(
                    &costs_cache,
                    &costs_auth,
                    e,
                    "monthly costs",
                    &spawn_app_handle,
                );
            }

            println!("[Refresh] slow data complete");
            {
                let snapshot = records_cache.get();

                // Rebuild history from daily_costs if needed
                records_history.rebuild_from_daily_costs(&snapshot);

                // Record today's entry
                records_history.record(&snapshot);

                // Refresh the dynamic tray icon + tooltip from the new snapshot.
                // Errors are non-fatal — a tray update must never break refresh.
                if let Some(ref app) = spawn_app_handle {
                    crate::tray_icon::update_tray(app, &snapshot);
                }

                // Budget projection & cost spike notifications
                let settings = spawn_settings.get();

                // Auto-generate report if configured
                if settings.report_auto_generate && settings.report_frequency != "off" {
                    let data_dir = crate::paths::get_data_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    if crate::report_generator::should_generate_report(
                        &settings.report_frequency,
                        &data_dir,
                    ) {
                        let history_entries = records_history.get_entries(90);
                        if let Err(e) = crate::report_generator::generate_usage_report(
                            &snapshot,
                            &history_entries,
                            &settings,
                            &settings.report_frequency,
                            &data_dir,
                        ) {
                            eprintln!("[Scheduler] Auto-report generation failed: {}", e);
                        }
                    }
                }

                // Auto-backup if enabled and today's backup doesn't exist
                if settings.auto_backup && crate::maintenance::should_auto_backup() {
                    let history_entries = records_history.get_entries(90);
                    match crate::maintenance::auto_backup(
                        settings.clone(),
                        history_entries,
                        snapshot.clone(),
                    ) {
                        Ok(Some(path)) => {
                            println!("[Scheduler] Auto-backup created: {}", path);
                        }
                        Ok(None) => {
                            // Today's backup already exists, skip
                        }
                        Err(e) => {
                            eprintln!("[Scheduler] Auto-backup failed: {}", e);
                        }
                    }
                }

                let is_quiet = settings.quiet_hours_enabled
                    && notification_rules::is_in_quiet_hours(
                        &settings.quiet_hours_start,
                        &settings.quiet_hours_end,
                    );
                let spawn_skip = spawn_skip_notifications;
                if !spawn_skip && !is_quiet {
                    let cooldown = settings.notification_cooldown_mins;
                    let now = Utc::now();
                    let now_str = now.format("%Y-%m-%d").to_string();
                    let now_ym = &now_str[..7]; // "YYYY-MM"

                    // Budget projection
                    if settings.notify_budget_projection && settings.monthly_budget > 0 {
                        let budget_usd = settings.monthly_budget as f64 / 100.0;
                        let daily_costs = &snapshot.daily_costs;
                        let month_cost: i64 = daily_costs
                            .iter()
                            .filter(|c| c.date.starts_with(now_ym))
                            .map(|c| c.total_cost)
                            .sum();
                        let month_cost_usd = month_cost as f64 / 100_000_000.0;

                        // Only notify if ACTUAL spending exceeds budget, not projected
                        let actual_pct = if budget_usd > 0.0 {
                            month_cost_usd / budget_usd * 100.0
                        } else {
                            0.0
                        };

                        if actual_pct >= 100.0 {
                            let key =
                                format!("budget_exceeded:{}:{}", records_workspace_id, now_ym);
                            if spawn_notify_rules.should_send(&key, cooldown) {
                                if let Some(ref handle) = spawn_app_handle {
                                    let _ = handle
                                        .notification()
                                        .builder()
                                        .title("Budget Exceeded")
                                        .body(format!(
                                            "Monthly spending has reached {:.0}% of budget (${:.2} / ${:.2}).",
                                            actual_pct, month_cost_usd, budget_usd
                                        ))
                                        .show();
                                    spawn_notify_rules.mark_sent(&key);
                                }
                            }
                        }
                    }

                    // Cost spike
                    if settings.notify_cost_spike {
                        let today_str = now.format("%Y-%m-%d").to_string();
                        let mut today_cost: i64 = 0;
                        let mut month_total: i64 = 0;
                        let mut day_count: i64 = 0;
                        for c in &snapshot.daily_costs {
                            if c.date.starts_with(now_ym) {
                                month_total += c.total_cost;
                                day_count += 1;
                                if c.date == today_str {
                                    today_cost += c.total_cost;
                                }
                            }
                        }
                        let avg = if day_count > 0 {
                            month_total as f64 / day_count as f64
                        } else {
                            0.0
                        };
                        let today_usd = today_cost as f64 / 100_000_000.0;
                        let avg_usd = avg / 100_000_000.0;
                        if today_usd >= avg_usd * 1.8 && today_usd >= 0.25 {
                            let key = format!("cost_spike:{}:{}", records_workspace_id, today_str);
                            if spawn_notify_rules.should_send(&key, cooldown) {
                                if let Some(ref handle) = spawn_app_handle {
                                    let _ = handle
                                        .notification()
                                        .builder()
                                        .title("Cost Spike")
                                        .body(format!(
                                            "Today ${:.2} vs daily avg ${:.2}.",
                                            today_usd, avg_usd
                                        ))
                                        .show();
                                    spawn_notify_rules.mark_sent(&key);
                                }
                            }
                        }
                    }
                }
            }
            records_cache.update_refresh_state(|rs| {
                rs.is_refreshing = false;
                rs.phase = "done".into();
                rs.last_finished_at = Some(Utc::now().to_rfc3339());
            });
            ctx.is_refreshing.store(false, Ordering::Release);
            if let Some(ref handle) = spawn_app_handle {
                let _ = handle.emit(
                    "refresh-complete",
                    serde_json::json!({ "status": "complete" }),
                );
            }
        });

        // First refresh done — subsequent refreshes may fire notifications
        ctx.skip_notifications.store(false, Ordering::Release);
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

        if total_new > 0 {
            println!(
                "[Refresh] records: +{} (total {})",
                total_new, total_fetched
            );
        }
        Ok(())
    }

    async fn refresh_monthly_costs(
        client: Arc<OpenCodeClient>,
        cache: Arc<AppCache>,
        cookies: Vec<crate::auth::CookieEntry>,
        workspace_id: String,
    ) -> Result<(), String> {
        let costs = client.fetch_monthly_costs(&cookies, &workspace_id).await?;
        cache.update_with(|snapshot| {
            if snapshot.workspace_id != workspace_id {
                return;
            }
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
            if snapshot.workspace_id != workspace_id {
                return;
            }
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
            // Don't clear workspaces - they're global to the user
        } else if snapshot.workspace_id.is_empty() {
            snapshot.workspace_id = workspace_id.to_string();
        }
    }

    fn handle_fetch_error(
        cache: &AppCache,
        auth_store: &AuthStore,
        error: String,
        label: &str,
        app_handle: &Option<tauri::AppHandle>,
    ) {
        if error == "AUTH_EXPIRED" || error == "REDIRECT_TO_LOGIN" {
            println!("[Refresh] Auth expired ({}), clearing cookies", label);
            auth_store.clear_cookies().ok();
            cache.set_error("Session expired. Please log in again.".into());
            if let Some(ref handle) = app_handle {
                let _ = handle.emit(
                    "auth-state-changed",
                    serde_json::json!({ "state": "expired" }),
                );
            }
        } else {
            println!("[Refresh] {} error: {}", label, error);
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
            return;
        }

        self.cache.update_refresh_state(|rs| {
            rs.is_refreshing = true;
            rs.phase = "auth".into();
            rs.last_started_at = Some(Utc::now().to_rfc3339());
            rs.last_error = None;
        });

        let app_handle = self
            .app_handle
            .lock()
            .map(|handle| handle.clone())
            .unwrap_or(None);

        let ctx = RefreshContext {
            client: self.client.clone(),
            cache: self.cache.clone(),
            auth_store: self.auth_store.clone(),
            history_store: self.history_store.clone(),
            settings_store: self.settings_store.clone(),
            notification_rules: self.notification_rules.clone(),
            threshold_notifications: self.threshold_notifications.clone(),
            is_refreshing: self.is_refreshing.clone(),
            threshold: self.threshold.clone(),
            consecutive_failures: self.consecutive_failures.clone(),
            skip_notifications: self.skip_notifications.clone(),
        };

        Self::do_refresh(ctx, app_handle).await;
    }

    /// Notify scheduler that window visibility changed.
    pub fn set_visible(&self, visible: bool) {
        self.is_visible.store(visible, Ordering::Relaxed);
    }
}

fn normalize_visible_interval(secs: u64) -> u64 {
    if secs == 0 {
        DEFAULT_VISIBLE_INTERVAL_SECS
    } else {
        secs.clamp(MIN_VISIBLE_INTERVAL_SECS, MAX_REFRESH_INTERVAL_SECS)
    }
}

fn normalize_hidden_interval(secs: u64) -> u64 {
    if secs == 0 {
        0
    } else {
        secs.clamp(MIN_HIDDEN_INTERVAL_SECS, MAX_REFRESH_INTERVAL_SECS)
    }
}

/// Calculate exponential backoff multiplier based on consecutive failures.
/// Returns 1 for 0-2 failures, then doubles for each additional failure, capped at 64x.
fn calculate_backoff(failures: u32) -> u64 {
    if failures < 3 {
        1
    } else {
        // 3 failures → 2x, 4 → 4x, 5 → 8x, 6 → 16x, 7 → 32x, 8+ → 64x
        2u64.pow((failures - 2).min(6))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_normalization_keeps_safe_bounds() {
        assert_eq!(normalize_visible_interval(0), DEFAULT_VISIBLE_INTERVAL_SECS);
        assert_eq!(normalize_visible_interval(1), MIN_VISIBLE_INTERVAL_SECS);
        assert_eq!(normalize_visible_interval(7200), MAX_REFRESH_INTERVAL_SECS);
        assert_eq!(normalize_hidden_interval(0), 0);
        assert_eq!(normalize_hidden_interval(1), MIN_HIDDEN_INTERVAL_SECS);
        assert_eq!(normalize_hidden_interval(7200), MAX_REFRESH_INTERVAL_SECS);
    }

    #[test]
    fn backoff_no_backoff_under_3_failures() {
        assert_eq!(calculate_backoff(0), 1);
        assert_eq!(calculate_backoff(1), 1);
        assert_eq!(calculate_backoff(2), 1);
    }

    #[test]
    fn backoff_doubles_per_failure() {
        assert_eq!(calculate_backoff(3), 2);
        assert_eq!(calculate_backoff(4), 4);
        assert_eq!(calculate_backoff(5), 8);
        assert_eq!(calculate_backoff(6), 16);
        assert_eq!(calculate_backoff(7), 32);
    }

    #[test]
    fn backoff_caps_at_64x() {
        assert_eq!(calculate_backoff(8), 64);
        assert_eq!(calculate_backoff(9), 64);
        assert_eq!(calculate_backoff(100), 64);
        assert_eq!(calculate_backoff(u32::MAX), 64);
    }

    #[test]
    fn backoff_applied_to_base_interval() {
        // 30s base × 64x max = 1920s = 32 minutes
        let base = DEFAULT_VISIBLE_INTERVAL_SECS;
        assert_eq!(base.saturating_mul(calculate_backoff(0)), 30);
        assert_eq!(base.saturating_mul(calculate_backoff(3)), 60);
        assert_eq!(base.saturating_mul(calculate_backoff(8)), 1920);
    }
}
