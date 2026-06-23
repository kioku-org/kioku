use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub company_id: String,
    pub company_name: String,
    pub company_slug: String,
    pub role: String,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignInRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterPersonalRequest {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterAdminRequest {
    pub company_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_slug: Option<String>,
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionCreateRequest {
    pub title: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "research".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub title: String,
    pub status: String,
    pub mode: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageCreateRequest {
    pub id: String,
    pub role: String,
    pub content: Vec<ContentPart>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: Vec<ContentPart>,
    pub timestamp: i64,
    #[serde(default)]
    pub token_usage: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub id: String,
    pub meeting_id: Option<String>,
    pub text: String,
    pub speaker: Option<String>,
    pub score: f64,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeetingIngestRequest {
    pub title: String,
    pub date: i64,
    #[serde(default = "default_duration")]
    pub duration_seconds: i32,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub transcript: Vec<TranscriptSegment>,
}

fn default_duration() -> i32 {
    0
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub speaker: String,
    pub text: String,
    #[serde(default)]
    pub start_time: Option<f64>,
    #[serde(default)]
    pub end_time: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub company_id: String,
    pub title: String,
    pub date: i64,
    pub duration_seconds: i32,
    pub participants: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageRecord {
    pub session_id: String,
    pub model: String,
    pub provider: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_cents: i64,
    pub session_count: i64,
    pub last_active_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyConfig {
    #[serde(default)]
    pub allowed_models: Vec<String>,
    pub default_provider: String,
    pub default_model: String,
    pub hivemind_enabled: bool,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeySet {
    pub provider: String,
    pub plain_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyOut {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub provider: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub ok: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub id: String,
    pub filename: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyAuthKeyOut {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}
