use crate::models::{AppDataSnapshot, HistoryEntry};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

const HISTORY_FILE: &str = "opencode-history.json";
const DEFAULT_KEEP_DAYS: u32 = 90;
const DATE_FORMAT: &str = "%Y-%m-%d";

pub struct HistoryStore {
    data: RwLock<Vec<HistoryEntry>>,
    history_path: PathBuf,
}

impl HistoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let history_path = data_dir.join(HISTORY_FILE);
        let data = std::fs::read_to_string(&history_path)
            .ok()
            .and_then(|content| {
                // Try to parse as new format first
                if let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&content) {
                    return Some(entries);
                }

                // Try to parse as old format (without workspace_id)
                #[derive(serde::Deserialize)]
                struct OldHistoryEntry {
                    date: String,
                    rolling_pct: u32,
                    weekly_pct: u32,
                    monthly_pct: u32,
                    total_cost: i64,
                    recorded_at: String,
                }

                if let Ok(old_entries) = serde_json::from_str::<Vec<OldHistoryEntry>>(&content) {
                    println!(
                        "[History] Migrating {} old entries without workspace_id",
                        old_entries.len()
                    );
                    // Migrate old entries by assigning empty workspace_id
                    // These will be visible across all workspaces until new data is recorded
                    return Some(
                        old_entries
                            .into_iter()
                            .map(|e| HistoryEntry {
                                date: e.date,
                                workspace_id: String::new(),
                                rolling_pct: e.rolling_pct,
                                weekly_pct: e.weekly_pct,
                                monthly_pct: e.monthly_pct,
                                total_cost: e.total_cost,
                                recorded_at: e.recorded_at,
                            })
                            .collect(),
                    );
                }

                None
            })
            .unwrap_or_default();

        Self {
            data: RwLock::new(data),
            history_path,
        }
    }

    /// Record today's snapshot into history. Updates existing entry for today or appends new.
    pub fn record(&self, snapshot: &AppDataSnapshot) {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let workspace_id = snapshot.workspace_id.clone();
        let total_cost: i64 = snapshot.daily_costs.iter()
            .filter(|c| c.date == today)
            .map(|c| c.total_cost)
            .sum();

        let entry = HistoryEntry {
            date: today.clone(),
            workspace_id: workspace_id.clone(),
            rolling_pct: snapshot.usage.rolling.usage_percent,
            weekly_pct: snapshot.usage.weekly.usage_percent,
            monthly_pct: snapshot.usage.monthly.usage_percent,
            total_cost,
            recorded_at: Utc::now().to_rfc3339(),
        };

        if let Ok(mut writer) = self.data.write() {
            // Update existing entry for today + workspace or push new
            if let Some(existing) = writer
                .iter_mut()
                .find(|e| e.date == today && e.workspace_id == workspace_id)
            {
                *existing = entry;
            } else {
                writer.push(entry);
            }

            // Align/correct history costs for all dates present in snapshot.daily_costs
            // This corrects any past recording errors for the current month.
            let mut cost_by_date: HashMap<String, i64> = HashMap::new();
            for cost in &snapshot.daily_costs {
                *cost_by_date.entry(cost.date.clone()).or_insert(0) += cost.total_cost;
            }

            for (date, cache_cost) in cost_by_date {
                if let Some(existing) = writer
                    .iter_mut()
                    .find(|e| e.date == date && e.workspace_id == workspace_id)
                {
                    if existing.total_cost != cache_cost {
                        println!(
                            "[History] Aligning cost for date {} in workspace {}: {} -> {}",
                            date, workspace_id, existing.total_cost, cache_cost
                        );
                        existing.total_cost = cache_cost;
                    }
                }
            }

            prune_and_sort_entries(&mut writer, Utc::now().date_naive(), DEFAULT_KEEP_DAYS);

            self.persist_locked(&writer);
        }
    }

    /// Get history entries for the last N days.
    pub fn get_entries(&self, days: u32) -> Vec<HistoryEntry> {
        self.get_entries_for_workspace(days, None)
    }

    /// Get history entries for the last N days, optionally filtered by workspace_id.
    pub fn get_entries_for_workspace(
        &self,
        days: u32,
        workspace_id: Option<&str>,
    ) -> Vec<HistoryEntry> {
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();

        self.data
            .read()
            .map(|reader| {
                reader
                    .iter()
                    .filter(|e| e.date >= cutoff)
                    .filter(|e| workspace_id.is_none_or(|wid| e.workspace_id == wid))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear all history entries from memory and disk.
    pub fn clear(&self) {
        if let Ok(mut writer) = self.data.write() {
            writer.clear();
            self.persist_locked(&writer);
        }
    }

    /// Rebuild history from the cache snapshot.
    ///
    /// Skips rebuilding if the workspace already has existing history entries.
    pub fn rebuild_from_daily_costs(&self, snapshot: &AppDataSnapshot) {
        if snapshot.daily_costs.is_empty() {
            return;
        }

        let workspace_id = &snapshot.workspace_id;

        // Skip if this workspace already has history entries
        if let Ok(reader) = self.data.read() {
            let has_existing = reader
                .iter()
                .any(|e| e.workspace_id.as_str() == workspace_id.as_str());
            if has_existing {
                return;
            }
        }

        const ROLLING_LIMIT_CENTS: i64 = 1200; // $12.00
        const WEEKLY_LIMIT_CENTS: i64 = 3000; // $30.00
        const MONTHLY_LIMIT_CENTS: i64 = 6000; // $60.00

        let workspace_id = &snapshot.workspace_id;
        println!(
            "[History] Rebuilding history for workspace {} ({} daily-cost entries, {} usage records)",
            workspace_id,
            snapshot.daily_costs.len(),
            snapshot.usage_records.len(),
        );

        // ---- aggregate daily costs by date ----
        let mut cost_by_date: HashMap<String, i64> = HashMap::new();
        for cost in &snapshot.daily_costs {
            *cost_by_date.entry(cost.date.clone()).or_insert(0) += cost.total_cost;
        }

        let mut dates: Vec<String> = cost_by_date.keys().cloned().collect();
        dates.sort();

        // ---- index usage records by (date, NaiveDateTime) for 5h windows ----
        let mut records_by_date: HashMap<String, Vec<(NaiveDateTime, i64)>> = HashMap::new();
        for rec in &snapshot.usage_records {
            if let Some((dt, date_key)) = parse_record_time(&rec.time_created) {
                records_by_date
                    .entry(date_key)
                    .or_default()
                    .push((dt, rec.cost));
            }
        }
        for recs in records_by_date.values_mut() {
            recs.sort_by_key(|a| a.0);
        }

        // Determine billing day-of-month from the current monthly reset timer.
        // Falls back to the 1st if the snapshot doesn't have a valid timer.
        let billing_day = billing_day_from_reset(snapshot.usage.monthly.reset_in_sec);
        println!(
            "[History] Monthly billing day = {} (reset_in_sec = {})",
            billing_day, snapshot.usage.monthly.reset_in_sec
        );

        // ---- rebuild entries ----
        if let Ok(mut writer) = self.data.write() {
            writer.retain(|e| e.workspace_id.as_str() != workspace_id.as_str());

            for date in &dates {
                let day_cost = cost_by_date.get(date).copied().unwrap_or(0);

                // ── 5h rolling (per-request sliding window) ──
                let rolling_pct =
                    max_5h_window_pct(&records_by_date, date, ROLLING_LIMIT_CENTS, day_cost);

                // ── weekly: Monday→Sunday fixed reset ──
                let weekly_pct = match week_monday(date) {
                    Some(monday) => {
                        let cost = sum_date_range(&cost_by_date, &monday, date);
                        compute_pct(cost, WEEKLY_LIMIT_CENTS)
                    }
                    None => compute_pct(day_cost, WEEKLY_LIMIT_CENTS),
                };

                // ── monthly: billing-cycle reset ──
                let monthly_pct = match billing_period_start(date, billing_day) {
                    Some(start) => {
                        let cost = sum_date_range(&cost_by_date, &start, date);
                        compute_pct(cost, MONTHLY_LIMIT_CENTS)
                    }
                    None => compute_pct(day_cost, MONTHLY_LIMIT_CENTS),
                };

                writer.push(HistoryEntry {
                    date: date.clone(),
                    workspace_id: workspace_id.clone(),
                    rolling_pct,
                    weekly_pct,
                    monthly_pct,
                    total_cost: day_cost,
                    recorded_at: Utc::now().to_rfc3339(),
                });
            }

            prune_and_sort_entries(&mut writer, Utc::now().date_naive(), DEFAULT_KEEP_DAYS);

            self.persist_locked(&writer);
            println!(
                "[History] Rebuild complete — {} total entries across all workspaces",
                writer.len()
            );
        }
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

fn sum_date_range(cost_by_date: &HashMap<String, i64>, start: &str, end: &str) -> i64 {
    let start_d = match NaiveDate::parse_from_str(start, DATE_FORMAT) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let end_d = match NaiveDate::parse_from_str(end, DATE_FORMAT) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    if start_d > end_d {
        return 0;
    }

    let mut total: i64 = 0;
    let mut d = start_d;
    while d <= end_d {
        let key = d.format(DATE_FORMAT).to_string();
        if let Some(cost) = cost_by_date.get(&key) {
            total += cost;
        }
        d += Duration::days(1);
    }
    total
}

fn compute_pct(cost: i64, limit_cents: i64) -> u32 {
    if limit_cents <= 0 {
        return 0;
    }
    ((cost as f64 / limit_cents as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u32
}

fn parse_record_time(ts: &str) -> Option<(NaiveDateTime, String)> {
    let trimmed = ts.trim();
    // Try RFC 3339 with timezone first, then fall back to naive + 'Z'
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        let naive = dt.naive_utc();
        let date_key = naive.format(DATE_FORMAT).to_string();
        return Some((naive, date_key));
    }
    // Strip trailing Z / z and try without timezone
    let stripped = trimmed.strip_suffix(['Z', 'z']).unwrap_or(trimmed);
    if let Ok(naive) = NaiveDateTime::parse_from_str(stripped, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(stripped, "%Y-%m-%dT%H:%M:%S"))
    {
        let date_key = naive.format(DATE_FORMAT).to_string();
        return Some((naive, date_key));
    }
    eprintln!("[History] Unparseable record timestamp: {}", ts);
    None
}

fn week_monday(date: &str) -> Option<String> {
    let d = NaiveDate::parse_from_str(date, DATE_FORMAT).ok()?;
    let days_from_monday = d.weekday().num_days_from_monday();
    Some(
        (d - Duration::days(days_from_monday as i64))
            .format(DATE_FORMAT)
            .to_string(),
    )
}

fn billing_day_from_reset(reset_in_sec: u64) -> u32 {
    if reset_in_sec == 0 {
        return 1;
    }
    let reset_seconds = reset_in_sec.min(i64::MAX as u64) as i64;
    let reset_dt = Utc::now() + Duration::seconds(reset_seconds);
    reset_dt.day()
}

fn billing_period_start(date: &str, billing_day: u32) -> Option<String> {
    let d = NaiveDate::parse_from_str(date, DATE_FORMAT).ok()?;
    let billing_day = billing_day.clamp(1, 31);
    let current_month_billing_day = billing_day.min(last_day_of_month(d.year(), d.month()));

    // The month whose billing date we need
    let (year, month) = if d.day() >= current_month_billing_day {
        (d.year(), d.month())
    } else {
        // Use previous month
        if d.month() == 1 {
            (d.year() - 1, 12)
        } else {
            (d.year(), d.month() - 1)
        }
    };

    // Clamp billing_day to the actual number of days in the month
    let days_in_month = last_day_of_month(year, month);
    let actual_day = billing_day.min(days_in_month);

    NaiveDate::from_ymd_opt(year, month, actual_day)
        .map(|start| start.format(DATE_FORMAT).to_string())
}

/// Number of days in a given month.
fn last_day_of_month(year: i32, month: u32) -> u32 {
    // Try the 1st of next month, subtract 1 day
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .map(|first_of_next| (first_of_next - Duration::days(1)).day())
    .unwrap_or(30)
}

/// Maximum 5h rolling-window usage percentage for a given date.
fn max_5h_window_pct(
    records_by_date: &HashMap<String, Vec<(NaiveDateTime, i64)>>,
    date: &str,
    limit_cents: i64,
    fallback_day_cost: i64,
) -> u32 {
    let today_start = match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => match d.and_hms_opt(0, 0, 0) {
            Some(dt) => dt,
            None => return compute_pct(fallback_day_cost, limit_cents),
        },
        Err(_) => return compute_pct(fallback_day_cost, limit_cents),
    };

    let prev_date = match NaiveDate::parse_from_str(date, DATE_FORMAT) {
        Ok(d) => (d - Duration::days(1)).format(DATE_FORMAT).to_string(),
        Err(_) => return compute_pct(fallback_day_cost, limit_cents),
    };

    let today_recs = records_by_date.get(date);
    let prev_recs = records_by_date.get(&prev_date);

    if today_recs.is_none() && prev_recs.is_none() {
        return compute_pct(fallback_day_cost, limit_cents);
    }

    let mut window_recs: Vec<&(NaiveDateTime, i64)> = Vec::new();
    if let Some(recs) = prev_recs {
        window_recs.extend(recs.iter());
    }
    if let Some(recs) = today_recs {
        window_recs.extend(recs.iter());
    }
    window_recs.sort_by_key(|a| a.0);

    if window_recs.is_empty() {
        return compute_pct(fallback_day_cost, limit_cents);
    }

    let five_hours = chrono::Duration::hours(5);
    let mut max_cost: i64 = 0;

    let mut j = 0usize;
    let mut window_sum: i64 = 0;

    for i in 0..window_recs.len() {
        if i > 0 {
            window_sum -= window_recs[i - 1].1;
        }
        while j < window_recs.len() && (window_recs[j].0 - window_recs[i].0) <= five_hours {
            window_sum += window_recs[j].1;
            j += 1;
        }

        if window_recs[i].0 + five_hours >= today_start {
            max_cost = max_cost.max(window_sum);
        }
    }

    compute_pct(max_cost, limit_cents)
}

#[cfg(test)]
mod history_store_tests {
    use super::{
        billing_period_start, compute_pct, max_5h_window_pct, parse_record_time, HistoryStore,
        HISTORY_FILE,
    };
    use crate::models::{AppDataSnapshot, DailyCostEntry};
    use chrono::{NaiveDate, Utc};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_data_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!(
            "opencode-history-{}-{}-{}",
            name,
            std::process::id(),
            millis
        ))
    }

    fn daily_cost(date: &str, cents: i64) -> DailyCostEntry {
        DailyCostEntry {
            date: date.into(),
            model: "model".into(),
            total_cost: cents,
            key_id: "key".into(),
            plan: None,
        }
    }

    #[test]
    fn legacy_history_without_workspace_id_is_migrated() {
        let dir = temp_data_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(HISTORY_FILE),
            r#"[{
                "date": "2026-01-02",
                "rolling_pct": 10,
                "weekly_pct": 20,
                "monthly_pct": 30,
                "total_cost": 123,
                "recorded_at": "2026-01-02T00:00:00Z"
            }]"#,
        )
        .unwrap();

        let store = HistoryStore::new(dir.clone());
        let entries = store.get_entries(36500);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].workspace_id, "");
        assert_eq!(entries[0].rolling_pct, 10);
        assert_eq!(entries[0].total_cost, 123);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn record_replaces_today_per_workspace_and_filters_by_workspace() {
        let dir = temp_data_dir("record");
        let store = HistoryStore::new(dir.clone());
        let today = Utc::now().format("%Y-%m-%d").to_string();

        let mut first = AppDataSnapshot::empty();
        first.workspace_id = "ws-a".into();
        first.usage.rolling.usage_percent = 10;
        first.usage.weekly.usage_percent = 20;
        first.usage.monthly.usage_percent = 30;
        first.daily_costs = vec![daily_cost(&today, 100)];
        store.record(&first);

        let mut replacement = first.clone();
        replacement.usage.rolling.usage_percent = 90;
        replacement.daily_costs = vec![daily_cost(&today, 250)];
        store.record(&replacement);

        let mut second_workspace = AppDataSnapshot::empty();
        second_workspace.workspace_id = "ws-b".into();
        second_workspace.usage.rolling.usage_percent = 40;
        second_workspace.daily_costs = vec![daily_cost(&today, 400)];
        store.record(&second_workspace);

        let ws_a = store.get_entries_for_workspace(1, Some("ws-a"));
        let ws_b = store.get_entries_for_workspace(1, Some("ws-b"));

        assert_eq!(ws_a.len(), 1);
        assert_eq!(ws_a[0].rolling_pct, 90);
        assert_eq!(ws_a[0].total_cost, 250);
        assert_eq!(ws_b.len(), 1);
        assert_eq!(ws_b[0].rolling_pct, 40);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compute_pct_clamps_and_handles_invalid_limits() {
        assert_eq!(compute_pct(0, 1200), 0);
        assert_eq!(compute_pct(149, 200), 75);
        assert_eq!(compute_pct(2000, 1200), 100);
        assert_eq!(compute_pct(-50, 1200), 0);
        assert_eq!(compute_pct(100, 0), 0);
    }

    #[test]
    fn parse_record_time_normalizes_offsets_to_utc_day() {
        let (_, date_key) = parse_record_time("2026-01-02T00:30:00+01:00").unwrap();
        assert_eq!(date_key, "2026-01-01");

        let (naive, z_date_key) = parse_record_time("2026-01-02T03:04:05Z").unwrap();
        assert_eq!(naive.date(), NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        assert_eq!(z_date_key, "2026-01-02");
        assert!(parse_record_time("not a timestamp").is_none());
    }

    #[test]
    fn billing_period_start_clamps_oversized_billing_day() {
        assert_eq!(
            billing_period_start("2024-03-01", 31).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(
            billing_period_start("2024-03-31", 31).as_deref(),
            Some("2024-03-31")
        );
        assert_eq!(billing_period_start("bad-date", 31), None);
    }

    #[test]
    fn max_5h_window_uses_previous_day_overlap_and_fallback() {
        let mut records_by_date = HashMap::new();
        records_by_date.insert(
            "2026-01-01".into(),
            vec![(
                NaiveDate::from_ymd_opt(2026, 1, 1)
                    .unwrap()
                    .and_hms_opt(23, 30, 0)
                    .unwrap(),
                600,
            )],
        );
        records_by_date.insert(
            "2026-01-02".into(),
            vec![
                (
                    NaiveDate::from_ymd_opt(2026, 1, 2)
                        .unwrap()
                        .and_hms_opt(1, 0, 0)
                        .unwrap(),
                    700,
                ),
                (
                    NaiveDate::from_ymd_opt(2026, 1, 2)
                        .unwrap()
                        .and_hms_opt(10, 0, 0)
                        .unwrap(),
                    100,
                ),
            ],
        );

        assert_eq!(
            max_5h_window_pct(&records_by_date, "2026-01-02", 1200, 100),
            100
        );
        assert_eq!(
            max_5h_window_pct(&HashMap::new(), "2026-01-02", 1200, 600),
            50
        );
    }
}

fn prune_and_sort_entries(entries: &mut Vec<HistoryEntry>, today: NaiveDate, keep_days: u32) {
    let cutoff = (today - Duration::days(keep_days as i64))
        .format(DATE_FORMAT)
        .to_string();

    entries.retain(|e| e.date >= cutoff);
    entries.sort_by(|a, b| {
        a.workspace_id
            .cmp(&b.workspace_id)
            .then_with(|| a.date.cmp(&b.date))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(date: &str, workspace_id: &str) -> HistoryEntry {
        HistoryEntry {
            date: date.to_string(),
            workspace_id: workspace_id.to_string(),
            rolling_pct: 0,
            weekly_pct: 0,
            monthly_pct: 0,
            total_cost: 0,
            recorded_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn sum_date_range_handles_invalid_and_reversed_ranges() {
        let mut costs = HashMap::new();
        costs.insert("2026-01-01".to_string(), 100);
        costs.insert("2026-01-02".to_string(), 200);

        assert_eq!(sum_date_range(&costs, "2026-01-01", "2026-01-02"), 300);
        assert_eq!(sum_date_range(&costs, "2026-01-03", "2026-01-01"), 0);
        assert_eq!(sum_date_range(&costs, "not-a-date", "2026-01-01"), 0);
    }

    #[test]
    fn billing_period_start_clamps_invalid_billing_days() {
        assert_eq!(
            billing_period_start("2026-03-10", 0),
            Some("2026-03-01".to_string())
        );
        assert_eq!(
            billing_period_start("2026-02-10", 31),
            Some("2026-01-31".to_string())
        );
        assert_eq!(
            billing_period_start("2026-02-28", 31),
            Some("2026-02-28".to_string())
        );
    }

    #[test]
    fn parse_record_time_accepts_timezone_and_naive_variants() {
        let with_timezone = parse_record_time("2026-01-02T03:04:05+08:00").unwrap();
        assert_eq!(with_timezone.1, "2026-01-01");

        let naive_z = parse_record_time("2026-01-02T03:04:05.123Z").unwrap();
        assert_eq!(naive_z.1, "2026-01-02");
    }

    #[test]
    fn prune_and_sort_entries_removes_old_entries_and_orders_by_workspace() {
        let mut entries = vec![
            entry("2026-01-03", "b"),
            entry("2025-12-01", "a"),
            entry("2026-01-01", "a"),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();

        prune_and_sort_entries(&mut entries, today, 30);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].workspace_id, "a");
        assert_eq!(entries[1].workspace_id, "b");
    }
}
