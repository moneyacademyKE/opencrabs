use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub working_directory: Option<String>,
    pub token_count: i64,
    pub total_cost: f64,
    pub created_at: String,
    pub updated_at: String,
    pub is_archived: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    pub content: String,
    pub sequence: i32,
    pub token_count: Option<i64>,
    pub cost: Option<f64>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub name: String,
    pub enabled: bool,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub providers: Vec<ProviderEntry>,
    pub agent_auto_approve: bool,
    pub a2a_enabled: bool,
    pub a2a_port: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrainFile {
    pub name: String,
    pub content: String,
    pub category: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDetail {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub parameters: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub review_gate: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    pub description: String,
    pub body: String,
    pub source: String,
    pub review_gate: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CronJobInfo {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub timezone: String,
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: String,
    pub auto_approve: bool,
    pub deliver_to: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub profile_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CronJobRunInfo {
    pub id: String,
    pub job_id: String,
    pub job_name: String,
    pub status: String,
    pub content: Option<String>,
    pub error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub name: String,
    pub display_name: String,
    pub enabled: bool,
    pub alive: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DashboardDataInfo {
    pub summary: SummaryInfo,
    pub daily: Vec<DailyInfo>,
    pub projects: Vec<ProjectInfo>,
    pub models: Vec<ModelInfo>,
    pub tools: Vec<UsageToolInfo>,
    pub activities: Vec<ActivityInfo>,
    pub cache: Option<CacheInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummaryInfo {
    pub total_tokens: i64,
    pub total_cost: f64,
    pub session_count: i64,
    pub call_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DailyInfo {
    pub date: String,
    pub tokens: i64,
    pub cost: f64,
    pub calls: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub project: String,
    pub cost: f64,
    pub tokens: i64,
    pub sessions: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model: String,
    pub tokens: i64,
    pub cost: f64,
    pub calls: i64,
    pub estimated: bool,
    pub variants: Vec<VariantInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VariantInfo {
    pub name: String,
    pub tokens: i64,
    pub cost: f64,
    pub calls: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageToolInfo {
    pub tool_name: String,
    pub call_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivityInfo {
    pub category: String,
    pub cost: f64,
    pub turns: i64,
    pub one_shot_pct: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CacheInfo {
    pub cache_hit_pct: f64,
    pub cached_tokens: i64,
    pub total_input_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub extension: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub is_binary: bool,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoiceConfigInfo {
    pub stt_enabled: bool,
    pub stt_provider: String,
    pub tts_enabled: bool,
    pub tts_provider: String,
    pub status: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub app_version: String,
    pub config_present: bool,
    pub database_present: bool,
    pub log_path: String,
    pub log_tail: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub release_notes: String,
    pub date: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopState {
    pub route: String,
    pub selected_session_id: Option<String>,
}
