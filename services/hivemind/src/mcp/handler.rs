use rmcp::model::{
    CallToolRequestParams, CallToolResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::vector::HivemindVectorStore;
use crate::repos::auth::validate_token;

fn json_schema(value: serde_json::Value) -> Arc<JsonObject> {
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}

#[derive(Clone)]
pub struct KiokuMcpService {
    pub db: PgPool,
    pub vector_store: Arc<HivemindVectorStore>,
    pub jwt_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SearchParams {
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct MeetingIdParams {
    meeting_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DocumentIdParams {
    document_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IngestSessionParams {
    title: String,
    content: String,
    tags: Option<Vec<String>>,
    date: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngestMeetingParams {
    title: String,
    date: i64,
    duration_seconds: Option<i32>,
    participants: Option<Vec<String>>,
    transcript: Vec<TranscriptSegmentParam>,
}

#[derive(Debug, Clone, Deserialize)]
struct TranscriptSegmentParam {
    speaker: String,
    text: String,
    start_time: Option<f64>,
    end_time: Option<f64>,
}

fn text_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![rmcp::model::Content::text(msg.into())])
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Option<&JsonObject>) -> Result<T, ErrorData> {
    let obj = args.ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;
    serde_json::from_value(serde_json::Value::Object(obj.clone()))
        .map_err(|e| ErrorData::invalid_params(format!("Invalid arguments: {}", e), None))
}

fn all_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "search",
            "Search the Kioku knowledge base for meeting transcripts and documents.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language query" },
                    "limit": { "type": "integer", "description": "Max results (default 6)" }
                },
                "required": ["query"]
            })),
        ),
        Tool::new(
            "meetings",
            "List meetings stored in Kioku.",
            json_schema(serde_json::json!({"type": "object", "properties": {}})),
        ),
        Tool::new(
            "transcript",
            "Get the full transcript for a specific meeting by UUID.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "meeting_id": { "type": "string", "description": "Meeting UUID" } },
                "required": ["meeting_id"]
            })),
        ),
        Tool::new(
            "meeting_get",
            "Get details of a specific meeting by UUID.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "meeting_id": { "type": "string", "description": "Meeting UUID" } },
                "required": ["meeting_id"]
            })),
        ),
        Tool::new(
            "documents",
            "List uploaded PDF documents.",
            json_schema(serde_json::json!({"type": "object", "properties": {}})),
        ),
        Tool::new(
            "document_delete",
            "Delete a PDF document and its embeddings.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "document_id": { "type": "string", "description": "Document UUID" } },
                "required": ["document_id"]
            })),
        ),
        Tool::new(
            "session",
            "Dump a coding or working session into the knowledge base. Pass the full raw content — conversation logs, diffs, notes, decisions, anything. It will be chunked and embedded automatically.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short title for the session" },
                    "content": { "type": "string", "description": "The full session content — paste everything: conversation, code diffs, decisions, notes. Will be chunked automatically." },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Topics or technologies (optional)" },
                    "date": { "type": "integer", "description": "Unix timestamp ms (defaults to now)" }
                },
                "required": ["title", "content"]
            })),
        ),
        Tool::new(
            "meeting",
            "Ingest a meeting transcript into the knowledge base.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "date": { "type": "integer", "description": "Unix timestamp ms" },
                    "duration_seconds": { "type": "integer" },
                    "participants": { "type": "array", "items": { "type": "string" } },
                    "transcript": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "speaker": { "type": "string" },
                                "text": { "type": "string" },
                                "start_time": { "type": "number" },
                                "end_time": { "type": "number" }
                            },
                            "required": ["speaker", "text"]
                        }
                    }
                },
                "required": ["title", "date", "transcript"]
            })),
        ),
    ]
}

impl ServerHandler for KiokuMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        let tool_list = all_tools();
        async move {
            Ok(ListToolsResult {
                tools: tool_list,
                meta: None,
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            let name = request.name.clone();
            let args = request.arguments.as_ref();

            match name.as_ref() {
                "search" => self.handle_search(args, &context).await,
                "meetings" => self.handle_list_meetings(&context).await,
                "transcript" => self.handle_get_transcript(args, &context).await,
                "meeting_get" => self.handle_get_meeting(args, &context).await,
                "documents" => self.handle_list_documents(&context).await,
                "document_delete" => self.handle_delete_document(args, &context).await,
                "session" => self.handle_ingest_session(args, &context).await,
                "meeting" => self.handle_ingest_meeting(args, &context).await,
                _ => Err(ErrorData::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >()),
            }
        }
    }
}

impl KiokuMcpService {
    async fn handle_search(&self, args: Option<&JsonObject>, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let params: SearchParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let limit = params.limit.unwrap_or(6).min(20).max(1);
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return Ok(text_result("Query is empty"));
        }

        match crate::services::knowledge::KnowledgeService::search(
            &self.db,
            &self.vector_store,
            company_id,
            &query,
            limit,
        )
        .await
        {
            Ok(results) => Ok(text_result(
                serde_json::to_string_pretty(&results).unwrap_or_default(),
            )),
            Err(e) => Ok(text_result(format!("Search failed: {}", e))),
        }
    }

    async fn handle_list_meetings(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let company_id = extract_company_id(context, self).await?;
        let repo = crate::repos::meeting::MeetingRepo::new(self.db.clone());
        match repo.list(company_id).await {
            Ok(meetings) => Ok(text_result(
                serde_json::to_string_pretty(&meetings).unwrap_or_default(),
            )),
            Err(e) => Ok(text_result(format!("Failed to list meetings: {}", e))),
        }
    }

    async fn handle_get_transcript(
        &self,
        args: Option<&JsonObject>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params: MeetingIdParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let meeting_id = Uuid::parse_str(&params.meeting_id).map_err(|e| {
            ErrorData::invalid_params(format!("Invalid meeting_id UUID: {}", e), None)
        })?;

        let repo = crate::repos::knowledge::KnowledgeRepo::new(self.db.clone());
        match repo.get_meeting_chunks(meeting_id, company_id).await {
            Ok(chunks) => Ok(text_result(
                serde_json::to_string_pretty(&chunks).unwrap_or_default(),
            )),
            Err(e) => Ok(text_result(format!("Failed to get transcript: {}", e))),
        }
    }

    async fn handle_get_meeting(
        &self,
        args: Option<&JsonObject>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params: MeetingIdParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let meeting_id = Uuid::parse_str(&params.meeting_id).map_err(|e| {
            ErrorData::invalid_params(format!("Invalid meeting_id UUID: {}", e), None)
        })?;

        let repo = crate::repos::meeting::MeetingRepo::new(self.db.clone());
        match repo.get(meeting_id, company_id).await {
            Ok(Some(meeting)) => Ok(text_result(
                serde_json::to_string_pretty(&meeting).unwrap_or_default(),
            )),
            Ok(None) => Ok(text_result("Meeting not found")),
            Err(e) => Ok(text_result(format!("Failed to get meeting: {}", e))),
        }
    }

    async fn handle_list_documents(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let company_id = extract_company_id(context, self).await?;
        let repo = crate::repos::knowledge::KnowledgeRepo::new(self.db.clone());
        match repo.list_documents(company_id).await {
            Ok(docs) => Ok(text_result(
                serde_json::to_string_pretty(&docs).unwrap_or_default(),
            )),
            Err(e) => Ok(text_result(format!("Failed to list documents: {}", e))),
        }
    }

    async fn handle_delete_document(
        &self,
        args: Option<&JsonObject>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params: DocumentIdParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let document_id = Uuid::parse_str(&params.document_id).map_err(|e| {
            ErrorData::invalid_params(format!("Invalid document_id UUID: {}", e), None)
        })?;

        let repo = crate::repos::knowledge::KnowledgeRepo::new(self.db.clone());
        match repo.get_document(document_id, company_id).await {
            Ok(Some(_doc)) => {
                if let Err(e) = self.vector_store.delete_for_document(document_id).await {
                    return Ok(text_result(format!("Failed to delete vectors: {}", e)));
                }
                if let Err(e) = repo.delete_document(document_id, company_id).await {
                    return Ok(text_result(format!("Failed to delete document: {}", e)));
                }
                Ok(text_result(format!("Document {} deleted", document_id)))
            }
            Ok(None) => Ok(text_result("Document not found")),
            Err(e) => Ok(text_result(format!("Error: {}", e))),
        }
    }

    async fn handle_ingest_meeting(
        &self,
        args: Option<&JsonObject>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params: IngestMeetingParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let user_id = extract_user_id(context, self).await?;

        let meeting_id = Uuid::new_v4();
        let segments: Vec<crate::types::TranscriptSegment> = params
            .transcript
            .into_iter()
            .map(|s| crate::types::TranscriptSegment {
                speaker: s.speaker,
                text: s.text,
                start_time: s.start_time.unwrap_or(0.0),
                end_time: s.end_time.unwrap_or(0.0),
            })
            .collect();

        let docs = crate::services::knowledge::KnowledgeService::chunk_transcript(
            &segments,
            meeting_id,
            params.date,
        );

        match crate::services::knowledge::KnowledgeService::ingest_documents(
            &self.db,
            &self.vector_store,
            company_id,
            meeting_id,
            &docs,
        )
        .await
        {
            Ok(()) => {
                let participants_json =
                    serde_json::to_value(&params.participants.unwrap_or_default())
                        .unwrap_or(serde_json::json!([]));
                let now = crate::util::now_ms();
                if let Err(e) = sqlx::query(
                    "INSERT INTO meetings (id, company_id, user_id, title, date, duration_seconds, participants, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                    .bind(meeting_id)
                    .bind(company_id)
                    .bind(user_id)
                    .bind(&params.title)
                    .bind(params.date)
                    .bind(params.duration_seconds.unwrap_or(0))
                    .bind(&participants_json)
                    .bind(now)
                    .execute(&self.db)
                    .await
                {
                    return Ok(text_result(format!("Ingested vectors but failed to create meeting record: {}", e)));
                }
                Ok(text_result(format!(
                    "Meeting {} ingested successfully with {} chunks",
                    meeting_id,
                    docs.len()
                )))
            }
            Err(e) => Ok(text_result(format!("Ingestion failed: {}", e))),
        }
    }

    async fn handle_ingest_session(
        &self,
        args: Option<&JsonObject>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params: IngestSessionParams = parse_args(args)?;
        let company_id = extract_company_id(context, self).await?;
        let user_id = extract_user_id(context, self).await?;

        if params.content.trim().is_empty() {
            return Ok(text_result("content is empty"));
        }

        let session_id = Uuid::new_v4();
        let now = crate::util::now_ms();
        let date = params.date.unwrap_or(now);
        let tags = serde_json::to_value(&params.tags.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));

        // Store session record (summary = first 500 chars of content for display)
        let preview: String = params.content.chars().take(500).collect();
        if let Err(e) = sqlx::query(
            "INSERT INTO coding_sessions (id, company_id, user_id, title, summary, decisions, tags, date, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(session_id)
        .bind(company_id)
        .bind(user_id)
        .bind(&params.title)
        .bind(&preview)
        .bind(serde_json::json!([]))
        .bind(&tags)
        .bind(date)
        .bind(now)
        .execute(&self.db)
        .await
        {
            return Ok(text_result(format!("Failed to store session: {}", e)));
        }

        // Chunk the raw content and embed all chunks
        let raw_chunks = crate::services::knowledge::split_text_paragraphs(&params.content, 400);
        let chunk_count = raw_chunks.len();

        let metadata_base = serde_json::json!({
            "chunk_type": "session",
            "session_id": session_id.to_string(),
            "timestamp": date,
        });

        let docs: Vec<langchain_rust::schemas::Document> = raw_chunks
            .iter()
            .map(|chunk| langchain_rust::schemas::Document {
                page_content: chunk.clone(),
                metadata: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("chunk_type".to_string(), serde_json::json!("session"));
                    m.insert("session_id".to_string(), serde_json::json!(session_id.to_string()));
                    m.insert("timestamp".to_string(), serde_json::json!(date));
                    m
                },
                score: 0.0,
            })
            .collect();

        // Persist chunk records to Postgres
        for chunk_text in &raw_chunks {
            let chunk_id = Uuid::new_v4();
            if let Err(e) = sqlx::query(
                "INSERT INTO knowledge_chunks (id, session_id, text, chunk_type, metadata, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(chunk_id)
            .bind(session_id)
            .bind(chunk_text)
            .bind("session")
            .bind(&metadata_base)
            .bind(now)
            .execute(&self.db)
            .await
            {
                return Ok(text_result(format!("Stored session but failed to persist chunk: {}", e)));
            }
        }

        if let Err(e) = self.vector_store.add_documents_for_company(company_id, &docs).await {
            return Ok(text_result(format!("Stored session but embedding failed: {}", e)));
        }

        Ok(text_result(format!(
            "Session \"{}\" stored (id: {}) — {} chunks embedded. Searchable via `search`.",
            params.title, session_id, chunk_count
        )))
    }
}

/// Extract Bearer token from the HTTP request parts stored in context extensions.
fn bearer_token_from_context(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<axum::http::request::Parts>()?;
    let auth = parts.headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    Some(auth.strip_prefix("Bearer ")?.to_string())
}

async fn resolve_claims_from_token(
    jwt_secret: &str,
    db: &PgPool,
    token: &str,
) -> Option<(Uuid, Uuid)> {
    use crate::repos::auth::validate_token;
    if let Ok(claims) = validate_token(jwt_secret, token) {
        return Some((claims.company_id, claims.user_id));
    }
    // API key (cmp_xxx) — look up in company_api_keys by prefix
    if token.starts_with("cmp_") && token.len() >= 12 {
        let prefix = &token[..12];
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT company_id, user_id FROM company_api_keys WHERE key_prefix = $1 LIMIT 1",
        )
        .bind(prefix)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
        return row;
    }
    None
}

async fn extract_company_id(
    context: &RequestContext<RoleServer>,
    service: &KiokuMcpService,
) -> Result<Uuid, ErrorData> {
    // 1. Try peer_info meta (set when client sends _meta.company_id in initialize)
    if let Some(info) = context.peer.peer_info() {
        if let Some(meta) = &info.meta {
            if let Some(val) = meta.get("company_id").and_then(|v| v.as_str()) {
                return Uuid::parse_str(val).map_err(|e| {
                    ErrorData::invalid_params(format!("Invalid company_id: {}", e), None)
                });
            }
        }
    }
    // 2. Fall back to JWT/API-key in the HTTP Authorization header
    if let Some(token) = bearer_token_from_context(context) {
        if let Some((company_id, _)) =
            resolve_claims_from_token(&service.jwt_secret, &service.db, &token).await
        {
            return Ok(company_id);
        }
    }
    Err(ErrorData::invalid_params(
        "Cannot determine company_id. Ensure you are authenticated with a valid Bearer token.",
        None,
    ))
}

async fn extract_user_id(
    context: &RequestContext<RoleServer>,
    service: &KiokuMcpService,
) -> Result<Uuid, ErrorData> {
    if let Some(info) = context.peer.peer_info() {
        if let Some(meta) = &info.meta {
            if let Some(val) = meta.get("user_id").and_then(|v| v.as_str()) {
                return Uuid::parse_str(val).map_err(|e| {
                    ErrorData::invalid_params(format!("Invalid user_id: {}", e), None)
                });
            }
        }
    }
    if let Some(token) = bearer_token_from_context(context) {
        if let Some((_, user_id)) =
            resolve_claims_from_token(&service.jwt_secret, &service.db, &token).await
        {
            return Ok(user_id);
        }
    }
    Err(ErrorData::invalid_params(
        "Cannot determine user_id. Ensure you are authenticated with a valid Bearer token.",
        None,
    ))
}

fn extract_company_id_from_args(args: Option<&JsonObject>) -> Result<Uuid, ErrorData> {
    let obj = args.ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;
    let cid = obj
        .get("company_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing company_id", None))?;
    Uuid::parse_str(cid)
        .map_err(|e| ErrorData::invalid_params(format!("Invalid company_id: {}", e), None))
}
