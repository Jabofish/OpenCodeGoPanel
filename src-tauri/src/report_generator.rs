use crate::models::{
    AppDataSnapshot, HistoryEntry, ReportCosts, ReportQuota, ReportTrends, UsageReport,
};
use crate::settings_store::AppSettings;
use std::path::Path;

const COST_UNITS_PER_USD: f64 = 100_000_000.0;

/// Generate a Markdown usage report for the given period and write it to disk.
/// Returns the Markdown string.
pub fn generate_usage_report(
    snapshot: &AppDataSnapshot,
    history: &[HistoryEntry],
    settings: &AppSettings,
    period: &str,
    data_dir: &Path,
) -> Result<String, String> {
    let now = chrono::Utc::now();
    let today = now.format("%Y-%m-%d").to_string();

    let (period_start, period_end) = compute_period_range(period, &today);

    // --- Quota ---
    let quota = ReportQuota {
        rolling_pct: snapshot.usage.rolling.usage_percent,
        weekly_pct: snapshot.usage.weekly.usage_percent,
        monthly_pct: snapshot.usage.monthly.usage_percent,
        rolling_status: snapshot.usage.rolling.status.clone(),
        weekly_status: snapshot.usage.weekly.status.clone(),
        monthly_status: snapshot.usage.monthly.status.clone(),
    };

    // --- Costs ---
    let period_costs: Vec<&crate::models::DailyCostEntry> = snapshot
        .daily_costs
        .iter()
        .filter(|c| c.date >= period_start && c.date <= period_end)
        .collect();

    let period_cost_units: i64 = period_costs.iter().map(|c| c.total_cost).sum();
    let period_cost_usd = period_cost_units as f64 / COST_UNITS_PER_USD;

    let days_in_period = if period == "daily" {
        1.0
    } else {
        let day_count = period_costs
            .iter()
            .map(|c| c.date.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();
        (day_count as f64).max(1.0)
    };
    let daily_avg_usd = period_cost_usd / days_in_period;

    // Projected monthly cost
    let now_y = now.format("%Y").to_string().parse::<i32>().unwrap_or(0);
    let now_m = now.format("%m").to_string().parse::<u32>().unwrap_or(1);
    let day_of_month = now.format("%d").to_string().parse::<u32>().unwrap_or(1);
    let days_in_month = days_in_month_of(now_y, now_m);
    let projected_monthly_usd = if day_of_month > 0 {
        daily_avg_usd * days_in_month as f64
    } else {
        0.0
    };

    let budget_usd = settings.monthly_budget as f64 / 100.0;
    let budget_pct = if budget_usd > 0.0 {
        (projected_monthly_usd / budget_usd) * 100.0
    } else {
        0.0
    };

    let costs = ReportCosts {
        period_cost_usd,
        daily_avg_usd,
        projected_monthly_usd,
        budget_usd,
        budget_pct,
    };

    // --- Models ---
    let models = snapshot.model_calls.models.clone();

    // --- Trends ---
    let trends = compute_trends(history, &period_start, &period_end);

    let report = UsageReport {
        period: period.to_string(),
        period_start: period_start.clone(),
        period_end: period_end.clone(),
        generated_at: now.to_rfc3339(),
        workspace_id: snapshot.workspace_id.clone(),
        quota,
        costs,
        models,
        trends,
    };

    let markdown = render_markdown(&report);

    // Write to disk
    let reports_dir = data_dir.join("reports");
    if let Err(e) = std::fs::create_dir_all(&reports_dir) {
        eprintln!("[Report] Failed to create reports dir: {}", e);
        return Err(format!("Failed to create reports directory: {}", e));
    }

    let filename = format!("{}-{}.md", period, today);
    let report_path = reports_dir.join(&filename);
    std::fs::write(&report_path, &markdown)
        .map_err(|e| format!("Failed to write report: {}", e))?;

    println!(
        "[Report] Generated {} report: {}",
        period,
        report_path.display()
    );

    Ok(markdown)
}

/// Check if a report for the given period and today already exists.
pub fn report_exists_today(period: &str, data_dir: &Path) -> bool {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let filename = format!("{}-{}.md", period, today);
    data_dir.join("reports").join(filename).exists()
}

/// Determine if a report should be generated now based on frequency and current time.
pub fn should_generate_report(frequency: &str, data_dir: &Path) -> bool {
    if frequency == "off" {
        return false;
    }

    if report_exists_today(frequency, data_dir) {
        return false;
    }

    let now = chrono::Utc::now();
    match frequency {
        "daily" => true, // Generate if not already done today
        "weekly" => now.format("%u").to_string() == "1", // Monday
        "monthly" => now.format("%d").to_string() == "01", // 1st of month
        _ => false,
    }
}

fn compute_period_range(period: &str, today: &str) -> (String, String) {
    match period {
        "daily" => (today.to_string(), today.to_string()),
        "weekly" => {
            // Last 7 days from today
            let end = today.to_string();
            let start = date_offset(today, -6);
            (start, end)
        }
        "monthly" => {
            // Current month: 1st to today
            let parts: Vec<&str> = today.split('-').collect();
            let start = if parts.len() >= 2 {
                format!("{}-{}-01", parts[0], parts[1])
            } else {
                today.to_string()
            };
            (start, today.to_string())
        }
        _ => (today.to_string(), today.to_string()),
    }
}

fn compute_trends(history: &[HistoryEntry], period_start: &str, period_end: &str) -> ReportTrends {
    // Split history into current and previous periods for comparison
    let current: Vec<&HistoryEntry> = history
        .iter()
        .filter(|e| e.date.as_str() >= period_start && e.date.as_str() <= period_end)
        .collect();

    // Compute previous period range (same length, just before current)
    let day_count = current
        .iter()
        .map(|e| e.date.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len()
        .max(1);

    let prev_end = date_offset(period_start, -1);
    let prev_start = date_offset(&prev_end, -(day_count as i64) + 1);

    let previous: Vec<&HistoryEntry> = history
        .iter()
        .filter(|e| e.date >= prev_start && e.date <= prev_end)
        .collect();

    let avg_cost_current: f64 = if current.is_empty() {
        0.0
    } else {
        current.iter().map(|e| e.total_cost as f64).sum::<f64>() / current.len() as f64
    };
    let avg_cost_previous: f64 = if previous.is_empty() {
        0.0
    } else {
        previous.iter().map(|e| e.total_cost as f64).sum::<f64>() / previous.len() as f64
    };

    let avg_quota_current: f64 = if current.is_empty() {
        0.0
    } else {
        current.iter().map(|e| e.rolling_pct as f64).sum::<f64>() / current.len() as f64
    };
    let avg_quota_previous: f64 = if previous.is_empty() {
        0.0
    } else {
        previous.iter().map(|e| e.rolling_pct as f64).sum::<f64>() / previous.len() as f64
    };

    ReportTrends {
        cost_direction: direction(avg_cost_current, avg_cost_previous),
        cost_change_pct: change_pct(avg_cost_current, avg_cost_previous),
        quota_direction: direction(avg_quota_current, avg_quota_previous),
        quota_change_pct: change_pct(avg_quota_current, avg_quota_previous),
    }
}

fn direction(current: f64, previous: f64) -> String {
    if previous == 0.0 && current == 0.0 {
        "stable".into()
    } else if current > previous * 1.05 {
        "up".into()
    } else if current < previous * 0.95 {
        "down".into()
    } else {
        "stable".into()
    }
}

fn change_pct(current: f64, previous: f64) -> f64 {
    if previous == 0.0 {
        if current == 0.0 {
            0.0
        } else {
            100.0
        }
    } else {
        ((current - previous) / previous) * 100.0
    }
}

fn render_markdown(r: &UsageReport) -> String {
    let period_label = match r.period.as_str() {
        "daily" => "Daily",
        "weekly" => "Weekly",
        "monthly" => "Monthly",
        other => other,
    };

    let mut md = String::new();
    md.push_str(&format!(
        "# OpenCode Usage Report — {} ({} to {})\n\n",
        period_label, r.period_start, r.period_end
    ));
    md.push_str(&format!("Generated: {}\n\n", r.generated_at));

    // Quota
    md.push_str("## Quota Status\n\n");
    md.push_str("| Period | Usage | Status |\n");
    md.push_str("|--------|-------|--------|\n");
    md.push_str(&format!(
        "| Rolling (5h) | {}% | {} |\n",
        r.quota.rolling_pct, r.quota.rolling_status
    ));
    md.push_str(&format!(
        "| Weekly | {}% | {} |\n",
        r.quota.weekly_pct, r.quota.weekly_status
    ));
    md.push_str(&format!(
        "| Monthly | {}% | {} |\n",
        r.quota.monthly_pct, r.quota.monthly_status
    ));
    md.push('\n');

    // Costs
    md.push_str("## Cost Analysis\n\n");
    md.push_str(&format!("- Period cost: ${:.4}\n", r.costs.period_cost_usd));
    md.push_str(&format!("- Daily average: ${:.4}\n", r.costs.daily_avg_usd));
    md.push_str(&format!(
        "- Projected monthly: ${:.2}\n",
        r.costs.projected_monthly_usd
    ));
    if r.costs.budget_usd > 0.0 {
        md.push_str(&format!(
            "- Budget: ${:.2} ({:.1}%)\n",
            r.costs.budget_usd, r.costs.budget_pct
        ));
    }
    md.push('\n');

    // Models
    if !r.models.is_empty() {
        md.push_str("## Model Usage\n\n");
        md.push_str("| Model | Calls | Share |\n");
        md.push_str("|-------|-------|-------|\n");
        for m in &r.models {
            md.push_str(&format!(
                "| {} | {} | {:.1}% |\n",
                m.name, m.calls, m.percentage
            ));
        }
        md.push('\n');
    }

    // Trends
    md.push_str("## Trends\n\n");
    md.push_str(&format!(
        "- Cost trend: {} ({:+.1}%)\n",
        r.trends.cost_direction, r.trends.cost_change_pct
    ));
    md.push_str(&format!(
        "- Quota trend: {} ({:+.1}%)\n",
        r.trends.quota_direction, r.trends.quota_change_pct
    ));

    md
}

fn date_offset(date_str: &str, days: i64) -> String {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return date_str.to_string();
    }
    let y: i32 = parts[0].parse().unwrap_or(2026);
    let m: u32 = parts[1].parse().unwrap_or(1);
    let d: u32 = parts[2].parse().unwrap_or(1);

    let date = chrono::NaiveDate::from_ymd_opt(y, m, d)
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let offset = chrono::Duration::days(days);
    let result = date + offset;
    result.format("%Y-%m-%d").to_string()
}

fn days_in_month_of(year: i32, month: u32) -> u32 {
    if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap())
    .signed_duration_since(
        chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
    )
    .num_days() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelCallCount;

    #[test]
    fn period_range_daily() {
        let (start, end) = compute_period_range("daily", "2026-06-19");
        assert_eq!(start, "2026-06-19");
        assert_eq!(end, "2026-06-19");
    }

    #[test]
    fn period_range_weekly() {
        let (start, end) = compute_period_range("weekly", "2026-06-19");
        assert_eq!(start, "2026-06-13");
        assert_eq!(end, "2026-06-19");
    }

    #[test]
    fn period_range_monthly() {
        let (start, end) = compute_period_range("monthly", "2026-06-19");
        assert_eq!(start, "2026-06-01");
        assert_eq!(end, "2026-06-19");
    }

    #[test]
    fn days_in_month() {
        assert_eq!(days_in_month_of(2026, 1), 31);
        assert_eq!(days_in_month_of(2026, 2), 28);
        assert_eq!(days_in_month_of(2024, 2), 29); // leap year
        assert_eq!(days_in_month_of(2026, 6), 30);
    }

    #[test]
    fn direction_stable_when_equal() {
        assert_eq!(direction(100.0, 100.0), "stable");
    }

    #[test]
    fn direction_up_when_increased() {
        assert_eq!(direction(120.0, 100.0), "up");
    }

    #[test]
    fn direction_down_when_decreased() {
        assert_eq!(direction(80.0, 100.0), "down");
    }

    #[test]
    fn change_pct_calculation() {
        let pct = change_pct(150.0, 100.0);
        assert!((pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn date_offset_positive() {
        assert_eq!(date_offset("2026-06-19", 1), "2026-06-20");
    }

    #[test]
    fn date_offset_negative() {
        assert_eq!(date_offset("2026-06-19", -6), "2026-06-13");
    }

    #[test]
    fn render_markdown_contains_sections() {
        let report = UsageReport {
            period: "daily".into(),
            period_start: "2026-06-19".into(),
            period_end: "2026-06-19".into(),
            generated_at: "2026-06-19T12:00:00Z".into(),
            workspace_id: "ws-test".into(),
            quota: ReportQuota {
                rolling_pct: 42,
                weekly_pct: 55,
                monthly_pct: 30,
                rolling_status: "ok".into(),
                weekly_status: "ok".into(),
                monthly_status: "ok".into(),
            },
            costs: ReportCosts {
                period_cost_usd: 1.23,
                daily_avg_usd: 1.23,
                projected_monthly_usd: 36.9,
                budget_usd: 60.0,
                budget_pct: 61.5,
            },
            models: vec![ModelCallCount {
                name: "GPT-4".into(),
                calls: 100,
                percentage: 80.0,
            }],
            trends: ReportTrends {
                cost_direction: "up".into(),
                cost_change_pct: 12.5,
                quota_direction: "stable".into(),
                quota_change_pct: 0.0,
            },
        };

        let md = render_markdown(&report);
        assert!(md.contains("Quota Status"));
        assert!(md.contains("Cost Analysis"));
        assert!(md.contains("Model Usage"));
        assert!(md.contains("Trends"));
        assert!(md.contains("GPT-4"));
        assert!(md.contains("42%"));
    }
}
