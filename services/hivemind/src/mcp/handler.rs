use rmcp::model::{CallToolResult, ServerInfo, Tool, JsonObject, CallToolRequestParams, PaginatedRequestParams, ListToolsResult};
use rmcp::service::RequestContext;
use rmcp::{ServerHandler, RoleServer, ErrorData};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::vector::HivemindVectorStore;

fn json_schema(value: serde_json::Value) -> Arc<JsonObject> {
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}

#[derive(Clone)]
pub struct CompanionMcpService {
    pub db: PgPool,
    pub vector_store: Arc<HivemindVectorStore>,
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
            "companion_search",
            "Search the Companion knowledge base for meeting transcripts and documents.",
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
            "companion_list_meetings",
            "List meetings stored in Companion.",
            json_schema(serde_json::json!({"type": "object", "properties": {}})),
        ),
        Tool::new(
            "companion_get_transcript",
            "Get the full transcript for a specific meeting by UUID.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "meeting_id": { "type": "string", "description": "Meeting UUID" } },
                "required": ["meeting_id"]
            })),
        ),
        Tool::new(
            "companion_get_meeting",
            "Get details of a specific meeting by UUID.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "meeting_id": { "type": "string", "description": "Meeting UUID" } },
                "required": ["meeting_id"]
            })),
        ),
        Tool::new(
            "companion_list_documents",
            "List uploaded PDF documents.",
            json_schema(serde_json::json!({"type": "object", "properties": {}})),
        ),
        Tool::new(
            "companion_delete_document",
            "Delete a PDF document and its embeddings.",
            json_schema(serde_json::json!({
                "type": "object",
                "properties": { "document_id": { "type": "string", "description": "Document UUID" } },
                "required": ["document_id"]
            })),
        ),
        Tool::new(
            "companion_ingest_meeting",
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

impl ServerHandler for CompanionMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>> + rmcp::service::MaybeSendFuture + '_ {
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
    ) -> impl std::future::Future<Output = Result<CallToolResult, ErrorData>> + rmcp::service::MaybeSendFuture + '_ {
        async move {
            let name = request.name.clone();
            let args = request.arguments.as_ref();

            match name.as_ref() {
                "companion_search" => self.handle_search(args).await,
                "companion_list_meetings" => self.handle_list_meetings(&context).await,
                "companion_get_transcript" => self.handle_get_transcript(args).await,
                "companion_get_meeting" => self.handle_get_meeting(args, &context).await,
                "companion_list_documents" => self.handle_list_documents(&context).await,
                "companion_delete_document" => self.handle_delete_document(args, &context).await,
                "companion_ingest_meeting" => self.handle_ingest_meeting(args, &context).await,
                _ => Err(ErrorData::method_not_found::<rmcp::model::CallToolRequestMethod>()),
            }
        }
    }
}

impl CompanionMcpService {
    async fn handle_search(&self, args: Option<&JsonObject>) -> Result<CallToolResult, ErrorData> {
        let params: SearchParams = parse_args(args)?;
        let company_id = extract_company_id_from_args(args)?;
        let limit = params.limit.unwrap_or(6).min(20).max(1);
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return Ok(text_result("Query is empty"));
        }

        match crate::services::knowledge::KnowledgeService::search(
            &self.db, &self.vector_store, company_id, &query, limit,
        ).await {
            Ok(results) => Ok(text_result(serde_json::to_string_pretty(&results).unwrap_or_default())),
            Err(e) => Ok(text_result(format!("Search failed: {}", e))),
        }
    }

    async fn handle_list_meetings(&self, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let company_id = extract_company_id(context)?;
        let repo = crate::repos::meeting::MeetingRepo::new(self.db.clone());
        match repo.list(company_id).await {
            Ok(meetings) => Ok(text_result(serde_json::to_string_pretty(&meetings).unwrap_or_default())),
            Err(e) => Ok(text_result(format!("Failed to list meetings: {}", e))),
        }
    }

    async fn handle_get_transcript(&self, args: Option<&JsonObject>) -> Result<CallToolResult, ErrorData> {
        let params: MeetingIdParams = parse_args(args)?;
        let company_id = extract_company_id_from_args(args)?;
        let meeting_id = Uuid::parse_str(&params.meeting_id)
            .map_err(|e| ErrorData::invalid_params(format!("Invalid meeting_id UUID: {}", e), None))?;

        let repo = crate::repos::knowledge::KnowledgeRepo::new(self.db.clone());
        match repo.get_meeting_chunks(meeting_id, company_id).await {
            Ok(chunks) => Ok(text_result(serde_json::to_string_pretty(&chunks).unwrap_or_default())),
            Err(e) => Ok(text_result(format!("Failed to get transcript: {}", e))),
        }
    }

    async fn handle_get_meeting(&self, args: Option<&JsonObject>, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let params: MeetingIdParams = parse_args(args)?;
        let company_id = extract_company_id(context)?;
        let meeting_id = Uuid::parse_str(&params.meeting_id)
            .map_err(|e| ErrorData::invalid_params(format!("Invalid meeting_id UUID: {}", e), None))?;

        let repo = crate::repos::meeting::MeetingRepo::new(self.db.clone());
        match repo.get(meeting_id, company_id).await {
            Ok(Some(meeting)) => Ok(text_result(serde_json::to_string_pretty(&meeting).unwrap_or_default())),
            Ok(None) => Ok(text_result("Meeting not found")),
            Err(e) => Ok(text_result(format!("Failed to get meeting: {}", e))),
        }
    }

    async fn handle_list_documents(&self, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let company_id = extract_company_id(context)?;
        let repo = crate::repos::knowledge::KnowledgeRepo::new(self.db.clone());
        match repo.list_documents(company_id).await {
            Ok(docs) => Ok(text_result(serde_json::to_string_pretty(&docs).unwrap_or_default())),
            Err(e) => Ok(text_result(format!("Failed to list documents: {}", e))),
        }
    }

    async fn handle_delete_document(&self, args: Option<&JsonObject>, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let params: DocumentIdParams = parse_args(args)?;
        let company_id = extract_company_id(context)?;
        let document_id = Uuid::parse_str(&params.document_id)
            .map_err(|e| ErrorData::invalid_params(format!("Invalid document_id UUID: {}", e), None))?;

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

    async fn handle_ingest_meeting(&self, args: Option<&JsonObject>, context: &RequestContext<RoleServer>) -> Result<CallToolResult, ErrorData> {
        let params: IngestMeetingParams = parse_args(args)?;
        let company_id = extract_company_id(context)?;
        let user_id = extract_user_id(context)?;

        let meeting_id = Uuid::new_v4();
        let segments: Vec<crate::types::TranscriptSegment> = params.transcript.into_iter().map(|s| {
            crate::types::TranscriptSegment {
                speaker: s.speaker,
                text: s.text,
                start_time: s.start_time.unwrap_or(0.0),
                end_time: s.end_time.unwrap_or(0.0),
            }
        }).collect();

        let docs = crate::services::knowledge::KnowledgeService::chunk_transcript(
            &segments, meeting_id, params.date,
        );

        match crate::services::knowledge::KnowledgeService::ingest_documents(
            &self.db, &self.vector_store, company_id, meeting_id, &docs,
        ).await {
            Ok(()) => {
                let participants_json = serde_json::to_value(&params.participants.unwrap_or_default()).unwrap_or(serde_json::json!([]));
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
                Ok(text_result(format!("Meeting {} ingested successfully with {} chunks", meeting_id, docs.len())))
            }
            Err(e) => Ok(text_result(format!("Ingestion failed: {}", e))),
        }
    }
}

fn extract_company_id(context: &RequestContext<RoleServer>) -> Result<Uuid, ErrorData> {
    if let Some(info) = context.peer.peer_info() {
        if let Some(meta) = &info.meta {
            if let Some(val) = meta.get("company_id").and_then(|v| v.as_str()) {
                return Uuid::parse_str(val)
                    .map_err(|e| ErrorData::invalid_params(format!("Invalid company_id: {}", e), None));
            }
        }
    }
    Err(ErrorData::invalid_params("Missing company_id in session metadata. Set _meta.company_id during initialization.", None))
}

fn extract_user_id(context: &RequestContext<RoleServer>) -> Result<Uuid, ErrorData> {
    if let Some(info) = context.peer.peer_info() {
        if let Some(meta) = &info.meta {
            if let Some(val) = meta.get("user_id").and_then(|v| v.as_str()) {
                return Uuid::parse_str(val)
                    .map_err(|e| ErrorData::invalid_params(format!("Invalid user_id: {}", e), None));
            }
        }
    }
    Err(ErrorData::invalid_params("Missing user_id in session metadata", None))
}

fn extract_company_id_from_args(args: Option<&JsonObject>) -> Result<Uuid, ErrorData> {
    let obj = args.ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;
    let cid = obj.get("company_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing company_id", None))?;
    Uuid::parse_str(cid).map_err(|e| ErrorData::invalid_params(format!("Invalid company_id: {}", e), None))
}