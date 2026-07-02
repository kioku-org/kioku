use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_slug: String,
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
    pub workspace_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_slug: Option<String>,
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
    pub workspace_id: String,
    pub user_id: String,
    pub title: String,
    pub status: String,
    pub mode: String,
    #[serde(default)]
    pub cwd: Option<String>,
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
    pub chunk: serde_json::Value,
    pub meeting: serde_json::Value,
    pub score: f64,
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
    pub workspace_id: String,
    pub title: String,
    pub date: i64,
    pub duration_seconds: i32,
    pub participants: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatus {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub native_meeting_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub normalized_status: Option<String>,
    #[serde(default)]
    pub meeting_id_from_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotStatusResponse {
    #[serde(default)]
    pub running_bots: Vec<BotStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub hivemind_enabled: bool,
    pub updated_at: i64,
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
pub struct WorkspaceApiKeyOut {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InviteCreateRequest {
    pub email: String,
    #[serde(default = "default_invite_role")]
    pub role: String,
}

fn default_invite_role() -> String {
    "member".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteOut {
    pub id: String,
    pub workspace_id: String,
    pub email: String,
    pub role: String,
    pub created_at: i64,
    pub used_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceOut {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub tier: String,
    /// The caller's role in this specific workspace.
    pub role: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceCreateRequest {
    pub name: String,
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceCreateResponse {
    pub workspace: WorkspaceOut,
    pub token: String,
}
