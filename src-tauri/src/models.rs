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
    pub input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    pub output_tokens: Option<u64>,
    #[serde(rename = "reasoningTokens")]
    pub reasoning_tokens: Option<u64>,
    #[serde(rename = "cacheReadTokens")]
    pub cache_read_tokens: Option<u64>,
    #[serde(rename = "cacheWrite5mTokens")]
    pub cache_write_5m_tokens: Option<u64>,
    #[serde(rename = "cacheWrite1hTokens")]
    pub cache_write_1h_tokens: Option<u64>,
    pub cost: i64,
    #[serde(rename = "keyID")]
    pub key_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub enrichment: Option<Enrichment>,
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

/// Daily history entry for trend tracking (persisted locally)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub date: String, // "YYYY-MM-DD"
    pub workspace_id: String,
    pub rolling_pct: u32,
    pub weekly_pct: u32,
    pub monthly_pct: u32,
    pub total_cost: i64,
    pub recorded_at: String, // ISO 8601
}

/// Workspace entry parsed from the workspace page HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub id: String,
    pub name: String,
    pub slug: Option<String>,
}

/// Current state of the backend refresh cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshState {
    pub is_refreshing: bool,
    pub phase: String,
    pub last_started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_error: Option<String>,
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
    /// Workspaces available to the current user
    pub workspaces: Vec<WorkspaceEntry>,
    /// Backend refresh cycle state
    pub refresh_state: RefreshState,
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
            workspaces: vec![],
            refresh_state: RefreshState {
                is_refreshing: false,
                phase: "idle".into(),
                last_started_at: None,
                last_finished_at: None,
                last_error: None,
            },
        }
    }
}

/// Status of local data files (P4)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDataStatus {
    pub data_dir: String,
    pub cache_bytes: u64,
    pub history_bytes: u64,
    pub settings_bytes: u64,
    pub auth_bytes: u64,
    pub export_bytes: u64,
    pub export_count: u32,
}

/// Health check result (P8)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub has_auth: bool,
    pub cache_ok: bool,
    pub settings_ok: bool,
    pub history_ok: bool,
    pub data_dir: String,
    pub data_dir_exists: bool,
    pub data_dir_available: bool,
    pub data_dir_error: Option<String>,
    pub cache_file: DataFileHealth,
    pub settings_file: DataFileHealth,
    pub history_file: DataFileHealth,
    pub auth_file: DataFileHealth,
    pub last_refresh_error: Option<String>,
}

/// Diagnostic details for one local data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataFileHealth {
    pub exists: bool,
    pub readable: bool,
    pub bytes: u64,
    pub error: Option<String>,
}
