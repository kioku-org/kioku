use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Auth ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProvisionRequest {
    pub email: String,
    pub name: String,
}

/// A single workspace membership, embedded in the JWT so `kioku ws <name>`
/// can switch the active workspace locally (via the X-Workspace-Id header)
/// without a network round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipClaim {
    pub workspace_id: Uuid,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: Uuid,
    /// Active workspace at token-issue time — the default when no
    /// X-Workspace-Id header is sent. Kept alongside `memberships` so every
    /// existing call site and handler that only cares about "my one
    /// workspace" keeps working unchanged.
    pub workspace_id: Uuid,
    pub role: String,
    #[serde(default)]
    pub memberships: Vec<MembershipClaim>,
    pub exp: i64,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAdminRequest {
    pub workspace_name: String,
    pub workspace_slug: Option<String>,
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterMemberRequest {
    pub email: String,
    pub name: String,
    pub password: String,
    pub workspace_slug: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterPersonalRequest {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SignInRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthSession {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub workspace_slug: String,
    pub role: String,
    pub token: String,
}

// ─── Workspace Config ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct WorkspaceConfigOut {
    pub workspace_id: Uuid,
    pub hivemind_enabled: bool,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceConfigPatch {
    pub hivemind_enabled: Option<bool>,
}

// ─── Members ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MemberOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub email: String,
    pub name: String,
    pub joined_at: i64,
}

// ─── Invites ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct InviteOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: String,
    pub created_at: i64,
    pub used_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct InviteCreate {
    pub email: String,
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "member".into()
}

// ─── Workspaces ─────────────────────────────────────────────────────────────
// A workspace is the shared knowledge pool for a project. A user can belong
// to (and switch between) several.

#[derive(Debug, Serialize)]
pub struct WorkspaceOut {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub tier: String,
    /// The caller's role in this specific workspace.
    pub role: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceCreate {
    pub name: String,
    pub slug: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceCreateResponse {
    pub workspace: WorkspaceOut,
    /// A fresh token whose `memberships` includes the new workspace — the
    /// old token's membership list is now stale.
    pub token: String,
}

// ─── Workspace API Keys (CLI auth) ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkspaceApiKeyCreate {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceApiKeyOut {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

// ─── Meetings ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct TranscriptSegment {
    pub speaker: String,
    pub text: String,
    #[serde(default)]
    pub start_time: f64,
    #[serde(default)]
    pub end_time: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MeetingIngestRequest {
    pub title: String,
    pub date: i64,
    #[serde(default)]
    pub duration_seconds: i32,
    #[serde(default)]
    pub participants: Vec<String>,
    pub transcript: Vec<TranscriptSegment>,
    pub vexa_meeting_id: Option<i32>,
    pub vexa_platform: Option<String>,
    pub vexa_native_meeting_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeetingOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub date: i64,
    pub duration_seconds: i32,
    pub participants: serde_json::Value,
    pub summary: Option<String>,
    pub created_at: i64,
    pub vexa_meeting_id: Option<i32>,
    pub vexa_platform: Option<String>,
    pub vexa_native_meeting_id: Option<String>,
}

// ─── Knowledge ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct KnowledgeSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    6
}

#[derive(Debug, Serialize)]
pub struct KnowledgeSearchResult {
    pub chunk: serde_json::Value,
    pub meeting: serde_json::Value,
    pub score: f64,
}

// ─── Knowledge Documents (PDF uploads) ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct KnowledgeDocumentOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub file_size: i64,
    pub status: String,
    pub chunk_count: i32,
    pub metadata: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Dump arbitrary raw content (conversation logs, diffs, notes) into the knowledge base for
/// later semantic search — distinct from `SessionCreateRequest` below, which tracks live
/// coding-agent sessions (title/cwd/mode) and has nothing to do with knowledge ingestion.
#[derive(Debug, Deserialize)]
pub struct KnowledgeSessionIngestRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub date: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeSessionIngestResponse {
    pub session_id: Uuid,
    pub chunk_count: usize,
}

// ─── Sessions ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SessionCreateRequest {
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionPatchRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SessionOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub status: String,
    pub cwd: Option<String>,
    pub mode: String,
    pub created_at: i64,
    pub updated_at: i64,
}

// ─── Messages ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MessageCreateRequest {
    pub id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct MessageOut {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub timestamp: i64,
}

// ─── Trace Steps ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TraceStepCreateRequest {
    pub id: Uuid,
    pub r#type: String,
    pub status: String,
    pub title: String,
    pub content: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
    pub is_error: Option<bool>,
    pub timestamp: i64,
    pub duration: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TraceStepPatchRequest {
    pub r#type: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
    pub is_error: Option<bool>,
    pub timestamp: Option<i64>,
    pub duration: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TraceStepOut {
    pub id: Uuid,
    pub session_id: Uuid,
    pub r#type: String,
    pub status: String,
    pub title: String,
    pub content: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
    pub is_error: Option<bool>,
    pub timestamp: i64,
    pub duration: Option<i64>,
}
