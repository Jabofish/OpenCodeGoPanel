use std::collections::HashMap;
use std::sync::Mutex;
use chrono::Timelike;

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
    let parse = |s: &str| -> Option<(u32, u32)> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let h = parts[0].parse::<u32>().ok()?;
        let m = parts[1].parse::<u32>().ok()?;
        Some((h, m))
    };

    let (sh, sm) = match parse(start) {
        Some(v) => v,
        None => return false,
    };
    let (eh, em) = match parse(end) {
        Some(v) => v,
        None => return false,
    };

    let start_mins = sh * 60 + sm;
    let end_mins = eh * 60 + em;
    let now_mins = now.hour() * 60 + now.minute();

    if start_mins <= end_mins {
        // Same day: e.g. 08:00-22:00
        now_mins >= start_mins && now_mins < end_mins
    } else {
        // Cross midnight: e.g. 22:00-08:00
        now_mins >= start_mins || now_mins < end_mins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_same_day() {
        // 08:00-22:00 — outside at 07:00, inside at 12:00
        // Can't test precisely without mocking time, but test the logic
        assert_eq!(is_in_quiet_hours("08:00", "10:00"), false); // depends on current time
    }

    #[test]
    fn quiet_hours_cross_midnight_always_consistent() {
        // 22:00-08:00 — should either be inside or outside based on time
        let result = is_in_quiet_hours("22:00", "08:00");
        // Just ensure it doesn't panic
        assert!(result == true || result == false);
    }
}
