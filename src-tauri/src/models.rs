use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub rolling: UsagePeriod,
    pub weekly: UsagePeriod,
    pub monthly: UsagePeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePeriod {
    pub status: String,
    /// 0 to 100
    pub usage_percent: u32,
    /// Seconds until quota reset
    pub reset_in_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallStats {
    pub models: Vec<ModelCallCount>,
    pub total_calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallCount {
    pub name: String,
    pub calls: u64,
    /// Percentage of total calls (0.0 - 100.0)
    pub percentage: f64,
}

/// Individual API call record parsed from HTML embedded data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    #[serde(rename = "workspaceID")]
    pub workspace_id: String,
    #[serde(rename = "timeCreated")]
    pub time_created: String,
    #[serde(rename = "timeUpdated")]
    pub time_updated: String,
    #[serde(rename = "timeDeleted")]
    pub time_deleted: Option<String>,
    pub model: String,
    pub provider: String,
    #[serde(rename = "inputTokens")]
    pub input_tokens: u64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: u64,
    #[serde(rename = "reasoningTokens")]
    pub reasoning_tokens: Option<u64>,
    #[serde(rename = "cacheReadTokens")]
    pub cache_read_tokens: u64,
    #[serde(rename = "cacheWrite5mTokens")]
    pub cache_write_5m_tokens: Option<u64>,
    #[serde(rename = "cacheWrite1hTokens")]
    pub cache_write_1h_tokens: Option<u64>,
    pub cost: i64,
    #[serde(rename = "keyID")]
    pub key_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub enrichment: Enrichment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrichment {
    pub plan: Option<String>,
}

/// Daily aggregated cost entry (from /_server endpoint or local aggregation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCostEntry {
    pub date: String,
    pub model: String,
    #[serde(rename = "totalCost")]
    pub total_cost: i64,
    #[serde(rename = "keyId")]
    pub key_id: String,
    pub plan: Option<String>,
}

/// Snapshot of all cached data sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDataSnapshot {
    pub usage: UsageInfo,
    pub model_calls: ModelCallStats,
    pub workspace_id: String,
    pub last_updated: String, // ISO 8601
    pub error: Option<String>,
    /// Individual usage records from the usage page (up to 50)
    pub usage_records: Vec<UsageRecord>,
    /// Daily aggregated cost data (from /_server or local aggregation)
    pub daily_costs: Vec<DailyCostEntry>,
}

impl AppDataSnapshot {
    pub fn empty() -> Self {
        Self {
            usage: UsageInfo {
                rolling: UsagePeriod {
                    status: "unknown".into(),
                    usage_percent: 0,
                    reset_in_sec: 0,
                },
                weekly: UsagePeriod {
                    status: "unknown".into(),
                    usage_percent: 0,
                    reset_in_sec: 0,
                },
                monthly: UsagePeriod {
                    status: "unknown".into(),
                    usage_percent: 0,
                    reset_in_sec: 0,
                },
            },
            model_calls: ModelCallStats {
                models: vec![],
                total_calls: 0,
            },
            workspace_id: String::new(),
            last_updated: String::new(),
            error: Some("Not yet loaded".into()),
            usage_records: vec![],
            daily_costs: vec![],
        }
    }
}
