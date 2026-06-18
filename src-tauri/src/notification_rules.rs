use chrono::Timelike;
use std::collections::HashMap;
use std::sync::Mutex;

const MINUTES_PER_DAY: u32 = 24 * 60;

/// State for time-based cooldown notifications (e.g., budget exceeded, cost spike)
pub struct NotificationRuleState {
    last_sent: Mutex<HashMap<String, String>>,
}

/// State for threshold-based notifications (only notify on new threshold levels)
pub struct ThresholdNotificationState {
    /// Maps (workspace_id:period) -> whether we've already notified about crossing the threshold
    notified: Mutex<HashMap<String, bool>>,
}

impl Default for NotificationRuleState {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ThresholdNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationRuleState {
    pub fn new() -> Self {
        Self {
            last_sent: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether a notification for `key` should be sent given the cooldown.
    /// `cooldown_mins` is the minimum minutes between notifications for this key.
    pub fn should_send(&self, key: &str, cooldown_mins: u32) -> bool {
        let now = chrono::Utc::now().timestamp();
        if let Ok(guard) = self.last_sent.lock() {
            if let Some(last_ts) = guard.get(key) {
                if let Ok(last) = last_ts.parse::<i64>() {
                    return (now - last) >= (cooldown_mins as i64) * 60;
                }
            }
            true
        } else {
            false
        }
    }

    /// Record that a notification was sent for `key` right now.
    pub fn mark_sent(&self, key: &str) {
        let now = chrono::Utc::now().timestamp().to_string();
        if let Ok(mut guard) = self.last_sent.lock() {
            guard.insert(key.to_string(), now);
        }
    }
}

impl ThresholdNotificationState {
    pub fn new() -> Self {
        Self {
            notified: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether a threshold notification should be sent.
    /// Only notifies when:
    /// 1. Current usage is above the configured threshold, AND
    /// 2. We haven't notified yet since the last drop below threshold
    ///
    /// Returns (should_notify, is_above_threshold)
    pub fn should_notify_threshold(
        &self,
        workspace_id: &str,
        period: &str,
        current_pct: u32,
        threshold: u32,
    ) -> (bool, bool) {
        let key = format!("{}:{}", workspace_id, period);

        if current_pct < threshold {
            // Not above threshold - clear the notified flag
            if let Ok(mut guard) = self.notified.lock() {
                guard.remove(&key);
            }
            return (false, false);
        }

        // Above threshold - check if we've already notified
        if let Ok(mut guard) = self.notified.lock() {
            if guard.contains_key(&key) {
                // Already notified - don't notify again
                return (false, true);
            }
            // First time crossing - notify and mark as notified
            guard.insert(key, true);
            return (true, true);
        }

        (false, true)
    }

    /// Reset all tracked state (e.g., when threshold setting changes)
    pub fn reset_all(&self) {
        if let Ok(mut guard) = self.notified.lock() {
            guard.clear();
        }
    }

    /// Reset tracked state for a specific workspace/period pair
    pub fn reset(&self, workspace_id: &str, period: &str) {
        let key = format!("{}:{}", workspace_id, period);
        if let Ok(mut guard) = self.notified.lock() {
            guard.remove(&key);
        }
    }
}

/// Check if current local time falls within quiet hours range.
/// Supports cross-midnight ranges (e.g., 22:00-08:00).
pub fn is_in_quiet_hours(start: &str, end: &str) -> bool {
    let now = chrono::Local::now().time();
    let now_mins = now.hour() * 60 + now.minute();
    is_in_quiet_hours_at(start, end, now_mins)
}

fn is_in_quiet_hours_at(start: &str, end: &str, now_mins: u32) -> bool {
    let start_mins = match parse_time_to_minutes(start) {
        Some(v) => v,
        None => return false,
    };
    let end_mins = match parse_time_to_minutes(end) {
        Some(v) => v,
        None => return false,
    };
    if now_mins >= MINUTES_PER_DAY || start_mins == end_mins {
        return false;
    }

    if start_mins <= end_mins {
        // Same day: e.g. 08:00-22:00
        now_mins >= start_mins && now_mins < end_mins
    } else {
        // Cross midnight: e.g. 22:00-08:00
        now_mins >= start_mins || now_mins < end_mins
    }
}

fn parse_time_to_minutes(value: &str) -> Option<u32> {
    let (hours, minutes) = value.trim().split_once(':')?;
    let hours = hours.parse::<u32>().ok()?;
    let minutes = minutes.parse::<u32>().ok()?;
    if hours >= 24 || minutes >= 60 {
        return None;
    }
    Some(hours * 60 + minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_same_day_is_deterministic() {
        assert!(!is_in_quiet_hours_at("08:00", "10:00", 7 * 60 + 59));
        assert!(is_in_quiet_hours_at("08:00", "10:00", 8 * 60));
        assert!(is_in_quiet_hours_at("08:00", "10:00", 9 * 60 + 59));
        assert!(!is_in_quiet_hours_at("08:00", "10:00", 10 * 60));
    }

    #[test]
    fn quiet_hours_cross_midnight_is_deterministic() {
        assert!(!is_in_quiet_hours_at("22:00", "08:00", 21 * 60 + 59));
        assert!(is_in_quiet_hours_at("22:00", "08:00", 22 * 60));
        assert!(is_in_quiet_hours_at("22:00", "08:00", 2 * 60));
        assert!(!is_in_quiet_hours_at("22:00", "08:00", 8 * 60));
    }

    #[test]
    fn quiet_hours_rejects_invalid_ranges() {
        assert_eq!(parse_time_to_minutes("23:59"), Some(23 * 60 + 59));
        assert_eq!(parse_time_to_minutes("24:00"), None);
        assert_eq!(parse_time_to_minutes("12:60"), None);
        assert!(!is_in_quiet_hours_at("08:00", "08:00", 8 * 60));
        assert!(!is_in_quiet_hours_at("bad", "08:00", 2 * 60));
        assert!(!is_in_quiet_hours_at("22:00", "08:00", MINUTES_PER_DAY));
    }
}
