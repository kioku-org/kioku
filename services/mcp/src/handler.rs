use crate::config::Config;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, JsonObject,
    ListPromptsResult, ListToolsResult, PaginatedRequestParams, Prompt, PromptArgument,
    PromptMessage, PromptMessageRole, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

fn schema(value: Value) -> Arc<JsonObject> {
    match value {
        Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}

fn text_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![rmcp::model::Content::text(msg.into())])
}

fn error_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![rmcp::model::Content::text(msg.into())])
}

fn or_none(s: &str) -> &str {
    if s.is_empty() {
        "(none)"
    } else {
        s
    }
}

fn parse_args<T: serde::de::DeserializeOwned + Default>(
    args: Option<&JsonObject>,
) -> Result<T, ErrorData> {
    match args {
        None => Ok(T::default()),
        Some(obj) => serde_json::from_value(Value::Object(obj.clone()))
            .map_err(|e| ErrorData::invalid_params(format!("Invalid arguments: {e}"), None)),
    }
}

#[derive(Clone)]
pub struct KiokuMcpService {
    pub http: reqwest::Client,
    pub config: Arc<Config>,
}

impl KiokuMcpService {
    async fn resolve_vexa_key(&self, token: &str) -> String {
        let url = format!("{}/vexa/token", self.config.hivemind_api_url);
        match self
            .http
            .get(&url)
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(v) = resp.json::<Value>().await {
                    if let Some(key) = v.get("vexa_api_key").and_then(|k| k.as_str()) {
                        return key.to_string();
                    }
                }
                token.to_string()
            }
            _ => token.to_string(),
        }
    }

    async fn gateway(
        &self,
        method: reqwest::Method,
        path: &str,
        vexa_key: &str,
        body: Option<Value>,
        query: &[(&str, String)],
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.config.kioku_api_url, path);
        let mut req = self
            .http
            .request(method, &url)
            .header("X-API-Key", vexa_key)
            .query(query);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(json!({}));
        if !status.is_success() {
            return Err(format!("HTTP {status}: {body}"));
        }
        Ok(body)
    }

    /// Faithful port of main.py's download_url rewrite in `get_recording_media_download`:
    /// relative URLs get the gateway base prefixed; internal `minio:`/`minio/` URLs get their
    /// host swapped for the gateway's so external callers can actually reach them.
    fn rewrite_download_url(&self, data: &mut Value) {
        let Some(dl) = data
            .get("download_url")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if dl.starts_with('/') {
            data["download_url"] = json!(format!("{}{}", self.config.kioku_api_url, dl));
        } else if dl.contains("minio:") || dl.contains("minio/") {
            if let (Ok(base), Ok(parsed)) = (
                url::Url::parse(&self.config.kioku_api_url),
                url::Url::parse(&dl),
            ) {
                let mut rewritten = base;
                rewritten.set_path(parsed.path());
                data["download_url"] = json!(rewritten.to_string());
            }
        }
    }

    async fn hivemind(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.config.hivemind_api_url, path);
        let mut req = self.http.request(method, &url).bearer_auth(token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(json!({}));
        if !status.is_success() {
            return Err(format!("HTTP {status}: {body}"));
        }
        Ok(body)
    }
}

/// Matches main.py's `get_api_key`: prefer `Authorization: Bearer <token>`, then a raw
/// `Authorization: <token>` value (back-compat), then fall back to `X-API-Key`.
fn bearer_token_from_context(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<axum::http::request::Parts>()?;
    if let Some(auth) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        return Some(auth.strip_prefix("Bearer ").unwrap_or(auth).to_string());
    }
    parts
        .headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn to_result(r: Result<Value, String>) -> CallToolResult {
    match r {
        Ok(v) => text_result(serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => error_result(e),
    }
}

// ---- Argument shapes (only the fields these tools actually use) ----

#[derive(Debug, Default, Deserialize)]
struct ParseMeetingLinkArgs {
    meeting_url: String,
}

#[derive(Debug, Default, Deserialize)]
struct RequestMeetingBotArgs {
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
struct MeetingPlatformIdArgs {
    meeting_id: String,
    #[serde(default = "default_platform")]
    meeting_platform: String,
}
fn default_platform() -> String {
    "google_meet".to_string()
}

#[derive(Debug, Default, Deserialize)]
struct ListRecordingsArgs {
    #[serde(default = "default_50")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    meeting_db_id: Option<i64>,
}
fn default_50() -> i64 {
    50
}

#[derive(Debug, Default, Deserialize)]
struct RecordingIdArgs {
    recording_id: i64,
}

#[derive(Debug, Default, Deserialize)]
struct RecordingMediaDownloadArgs {
    recording_id: i64,
    media_file_id: i64,
}

#[derive(Debug, Default, Deserialize)]
struct RecordingConfigUpdateArgs {
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct MeetingBundleArgs {
    meeting_id: String,
    #[serde(default = "default_platform")]
    meeting_platform: String,
    #[serde(default)]
    include_segments: bool,
    #[serde(default = "default_true")]
    include_recordings: bool,
    #[serde(default = "default_true")]
    include_share_link: bool,
    #[serde(default)]
    include_media_download_urls: bool,
    share_ttl_seconds: Option<i64>,
}

// #[derive(Default)] would give include_recordings/include_share_link `false`, which is wrong
// for `parse_args`'s `None` (no-arguments-object) case — Python's defaults are both `True`.
impl Default for MeetingBundleArgs {
    fn default() -> Self {
        Self {
            meeting_id: String::new(),
            meeting_platform: default_platform(),
            include_segments: false,
            include_recordings: true,
            include_share_link: true,
            include_media_download_urls: false,
            share_ttl_seconds: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ShareLinkArgs {
    meeting_id: String,
    meeting_platform: String,
    meeting_db_id: Option<i64>,
    ttl_seconds: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct UpdateBotConfigArgs {
    meeting_id: String,
    #[serde(default = "default_platform")]
    meeting_platform: String,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
struct ListMeetingsArgs {
    #[serde(default = "default_20")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    status: Option<String>,
    platform: Option<String>,
}
fn default_20() -> i64 {
    20
}

#[derive(Debug, Default, Deserialize)]
struct UpdateMeetingDataArgs {
    meeting_id: String,
    #[serde(default = "default_platform")]
    meeting_platform: String,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchArgs {
    query: String,
    limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct MeetingIdArgs {
    meeting_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct DocumentIdArgs {
    document_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct IngestSessionArgs {
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
struct IngestMeetingArgs {
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

fn prompt_arg(name: &str, description: &str, required: bool) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(required)
}

fn all_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "vexa.meeting_prep",
            Some("Parse link, request bot, and attach meeting notes/metadata."),
            Some(vec![
                prompt_arg(
                    "meeting_url",
                    "Full meeting URL (recommended for Teams/Zoom).",
                    false,
                ),
                prompt_arg(
                    "meeting_platform",
                    "google_meet | teams | zoom (optional if meeting_url is provided).",
                    false,
                ),
                prompt_arg(
                    "meeting_id",
                    "Native meeting ID (optional if meeting_url is provided).",
                    false,
                ),
                prompt_arg(
                    "notes",
                    "Optional notes/agenda/context to store on the meeting.",
                    false,
                ),
            ]),
        )
        .with_title("Kioku: Meeting Prep"),
        Prompt::new(
            "vexa.during_meeting",
            Some("Check bot status and retrieve current transcript snapshot."),
            Some(vec![
                prompt_arg("meeting_platform", "google_meet | teams | zoom", true),
                prompt_arg("meeting_id", "Native meeting ID", true),
            ]),
        )
        .with_title("Kioku: During Meeting"),
        Prompt::new(
            "vexa.post_meeting",
            Some("Fetch bundle (notes, recordings, share link) and produce follow-ups."),
            Some(vec![
                prompt_arg("meeting_platform", "google_meet | teams | zoom", true),
                prompt_arg("meeting_id", "Native meeting ID", true),
            ]),
        )
        .with_title("Kioku: Post Meeting"),
        Prompt::new(
            "vexa.teams_link_help",
            Some("Supported Teams links and passcode requirements (issues #105/#110)."),
            Some(vec![prompt_arg(
                "meeting_url",
                "Teams meeting URL from the user",
                false,
            )]),
        )
        .with_title("Kioku: Teams Link Help"),
    ]
}

fn prompt_text(text: impl Into<String>) -> PromptMessage {
    PromptMessage::new_text(PromptMessageRole::User, text.into())
}

fn all_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "parse_meeting_link",
            "Parse a meeting URL into platform/native_meeting_id/passcode.",
            schema(json!({"type":"object","properties":{"meeting_url":{"type":"string"}},"required":["meeting_url"]})),
        ),
        Tool::new(
            "request_meeting_bot",
            "Request a Kioku bot to join a meeting for transcription. Accepts meeting_platform, native_meeting_id (or meeting_url), language, bot_name, passcode.",
            schema(json!({"type":"object","properties":{
                "meeting_platform":{"type":"string"},"native_meeting_id":{"type":"string"},
                "meeting_url":{"type":"string"},"language":{"type":"string"},
                "bot_name":{"type":"string"},"passcode":{"type":"string"}
            }})),
        ),
        Tool::new(
            "get_meeting_transcript",
            "Get the real-time transcript for a meeting.",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"},"meeting_platform":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "list_recordings",
            "List recordings for the authenticated user.",
            schema(json!({"type":"object","properties":{"limit":{"type":"integer"},"offset":{"type":"integer"},"meeting_db_id":{"type":"integer"}}})),
        ),
        Tool::new(
            "get_recording",
            "Get a single recording and its media files.",
            schema(json!({"type":"object","properties":{"recording_id":{"type":"integer"}},"required":["recording_id"]})),
        ),
        Tool::new(
            "delete_recording",
            "Delete a recording and its media files.",
            schema(json!({"type":"object","properties":{"recording_id":{"type":"integer"}},"required":["recording_id"]})),
        ),
        Tool::new(
            "get_recording_media_download",
            "Get a download URL for a recording media file.",
            schema(json!({"type":"object","properties":{"recording_id":{"type":"integer"},"media_file_id":{"type":"integer"}},"required":["recording_id","media_file_id"]})),
        ),
        Tool::new(
            "get_recording_config",
            "Get recording configuration for the authenticated user.",
            schema(json!({"type":"object","properties":{}})),
        ),
        Tool::new(
            "update_recording_config",
            "Update recording configuration for the authenticated user.",
            schema(json!({"type":"object","properties":{}})),
        ),
        Tool::new(
            "get_meeting_bundle",
            "Compact post-meeting bundle: transcript + notes + recordings + optional share link.",
            schema(json!({"type":"object","properties":{
                "meeting_id":{"type":"string"},"meeting_platform":{"type":"string"},
                "include_segments":{"type":"boolean"},"include_recordings":{"type":"boolean"},
                "include_share_link":{"type":"boolean"},"share_ttl_seconds":{"type":"integer"},
                "include_media_download_urls":{"type":"boolean"}
            },"required":["meeting_id"]})),
        ),
        Tool::new(
            "create_transcript_share_link",
            "Create a short-lived public URL for a transcript.",
            schema(json!({"type":"object","properties":{
                "meeting_id":{"type":"string"},"meeting_platform":{"type":"string"},
                "meeting_db_id":{"type":"integer"},"ttl_seconds":{"type":"integer"}
            },"required":["meeting_id","meeting_platform"]})),
        ),
        Tool::new(
            "get_bot_status",
            "Get the status of currently running bots.",
            schema(json!({"type":"object","properties":{}})),
        ),
        Tool::new(
            "update_bot_config",
            "Update the configuration of an active bot (e.g. changing the language).",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"},"meeting_platform":{"type":"string"},"language":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "stop_bot",
            "Remove an active bot from a meeting.",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"},"meeting_platform":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "list_meetings",
            "List meetings associated with your API key. Supports pagination and status/platform filters.",
            schema(json!({"type":"object","properties":{"limit":{"type":"integer"},"offset":{"type":"integer"},"status":{"type":"string"},"platform":{"type":"string"}}})),
        ),
        Tool::new(
            "update_meeting_data",
            "Update meeting metadata (name, participants, languages, notes).",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"},"meeting_platform":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "delete_meeting",
            "Purge transcripts and anonymize meeting data for a finalized meeting.",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"},"meeting_platform":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "search",
            "Search the Kioku knowledge base for meeting transcripts and documents.",
            schema(json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]})),
        ),
        Tool::new(
            "meetings",
            "List meetings stored in Kioku's knowledge base.",
            schema(json!({"type":"object","properties":{}})),
        ),
        Tool::new(
            "transcript",
            "Get the full knowledge-base transcript for a specific meeting by UUID.",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "meeting_get",
            "Get details of a specific knowledge-base meeting by UUID.",
            schema(json!({"type":"object","properties":{"meeting_id":{"type":"string"}},"required":["meeting_id"]})),
        ),
        Tool::new(
            "documents",
            "List uploaded knowledge-base documents.",
            schema(json!({"type":"object","properties":{}})),
        ),
        Tool::new(
            "document_delete",
            "Delete a knowledge-base document and its embeddings.",
            schema(json!({"type":"object","properties":{"document_id":{"type":"string"}},"required":["document_id"]})),
        ),
        Tool::new(
            "session",
            "Dump a coding/working session into the knowledge base for later search.",
            schema(json!({"type":"object","properties":{"title":{"type":"string"},"content":{"type":"string"},"tags":{"type":"array","items":{"type":"string"}},"date":{"type":"integer"}},"required":["title","content"]})),
        ),
        Tool::new(
            "meeting",
            "Ingest a meeting transcript into the knowledge base.",
            schema(json!({"type":"object","properties":{"title":{"type":"string"},"date":{"type":"integer"},"duration_seconds":{"type":"integer"},"participants":{"type":"array","items":{"type":"string"}},"transcript":{"type":"array"}},"required":["title","date","transcript"]})),
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
        async move {
            Ok(ListToolsResult {
                tools: all_tools(),
                meta: None,
                next_cursor: None,
            })
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            Ok(ListPromptsResult {
                prompts: all_prompts(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, ErrorData>>
           + rmcp::service::MaybeSendFuture
           + '_ {
        async move {
            let args = request.arguments.unwrap_or_default();
            let get = |k: &str| {
                args.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };

            match request.name.as_str() {
                "vexa.meeting_prep" => {
                    let (meeting_url, meeting_platform, meeting_id, notes) = (
                        get("meeting_url"),
                        get("meeting_platform"),
                        get("meeting_id"),
                        get("notes"),
                    );
                    Ok(GetPromptResult::new(vec![prompt_text(format!(
                            "You are helping me prepare a meeting using Kioku.\n\n\
                             Goals:\n1. Identify meeting platform + native meeting id (+ passcode if needed).\n\
                             2. Request the meeting bot (idempotent).\n\
                             3. Store meeting notes/metadata so it appears in transcript responses.\n\n\
                             Rules:\n- Prefer calling `parse_meeting_link` when `meeting_url` is provided.\n\
                             - For Teams: only `teams.live.com/meet/<id>?p=<passcode>` is supported; \
                             `teams.microsoft.com/l/meetup-join/...` is not supported.\n\
                             - When requesting a bot, pass `meeting_url` if you have it; otherwise use \
                             `native_meeting_id` (+ `passcode` for Teams, from ?p=).\n\
                             - After the meeting exists, call `update_meeting_data` with `notes` if provided.\n\n\
                             Input:\n- meeting_url: {}\n- meeting_platform: {}\n- meeting_id: {}\n- notes: {}\n\n\
                             Now do the tool calls and tell me what you did and what to do next.",
                            or_none(&meeting_url), or_none(&meeting_platform), or_none(&meeting_id), or_none(&notes)
                        ))]).with_description("Meeting prep flow using Kioku MCP tools.".to_string()))
                }
                "vexa.during_meeting" => {
                    let (meeting_platform, meeting_id) =
                        (get("meeting_platform"), get("meeting_id"));
                    Ok(GetPromptResult::new(vec![prompt_text(format!(
                            "You are my during-meeting assistant using Kioku.\n\n\
                             Meeting: platform={meeting_platform}, id={meeting_id}\n\n\
                             Steps:\n- Call `get_bot_status` to confirm the bot is active / requested.\n\
                             - Call `get_meeting_transcript` to fetch the current transcript snapshot.\n\
                             - If the transcript is empty, explain whether the meeting may not have started, \
                             bot may not be admitted yet, or transcription isn't producing segments.\n\n\
                             Then summarize key points and action items so far."
                        ))]).with_description("During-meeting helper prompt using Kioku MCP tools.".to_string()))
                }
                "vexa.post_meeting" => {
                    let (meeting_platform, meeting_id) =
                        (get("meeting_platform"), get("meeting_id"));
                    Ok(GetPromptResult::new(vec![prompt_text(format!(
                            "You are my post-meeting assistant using Kioku.\n\n\
                             Meeting: platform={meeting_platform}, id={meeting_id}\n\n\
                             Steps:\n- Call `get_meeting_bundle` (segments off) to fetch meeting status, notes, recordings, and share link.\n\
                             - If recordings exist, resolve download URLs if needed.\n\
                             - Produce:\n  1) concise summary\n  2) decisions\n\
                             3) action items with owners (if known) and due dates (if mentioned)\n  4) open questions\n"
                        ))]).with_description("Post-meeting helper prompt using Kioku MCP tools.".to_string()))
                }
                "vexa.teams_link_help" => {
                    let meeting_url = get("meeting_url");
                    Ok(GetPromptResult::new(vec![prompt_text(format!(
                            "Help me troubleshoot a Microsoft Teams meeting link for Kioku.\n\n\
                             User link: {}\n\n\
                             Checklist:\n- If link is `teams.live.com/meet/<id>?p=<passcode>`:\n\
                             - native_meeting_id = <id> (10-15 digits)\n\
                             - passcode = value of ?p= (often required)\n\
                             - Prefer using `meeting_url` directly with `request_meeting_bot`.\n\
                             - If link is `teams.microsoft.com/l/meetup-join/...`: explain it's not supported yet (issues #105/#110).\n\
                             - If passcode fails validation, explain constraints (8-20 alphanumeric) and ask for a corrected link.\n\n\
                             If a link is provided, call `parse_meeting_link` and show the extracted fields.",
                            if meeting_url.is_empty() { "(none provided)".to_string() } else { meeting_url }
                        ))]).with_description("Teams link troubleshooting prompt.".to_string()))
                }
                other => Err(ErrorData::invalid_params(
                    format!("Unknown prompt: {other}"),
                    None,
                )),
            }
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
            let Some(token) = bearer_token_from_context(&context) else {
                return Ok(error_result(
                    "Missing credentials (send Authorization: Bearer <token>).",
                ));
            };

            match name.as_ref() {
                "parse_meeting_link" => {
                    let p: ParseMeetingLinkArgs = parse_args(args)?;
                    Ok(match parse_meeting_url(&p.meeting_url) {
                        Ok(r) => text_result(serde_json::to_string_pretty(&r).unwrap_or_default()),
                        Err(e) => error_result(e),
                    })
                }
                "request_meeting_bot" => {
                    let p: RequestMeetingBotArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let mut payload = p.rest;
                    if let Some(Value::String(url)) = payload.remove("meeting_url") {
                        match parse_meeting_url(&url) {
                            Ok(parsed) => {
                                payload.insert("platform".into(), json!(parsed.platform));
                                payload.insert(
                                    "native_meeting_id".into(),
                                    json!(parsed.native_meeting_id),
                                );
                                payload
                                    .entry("passcode".to_string())
                                    .or_insert(json!(parsed.passcode));
                                if let Some(mu) = parsed.meeting_url {
                                    payload.insert("meeting_url".into(), json!(mu));
                                }
                                if let Some(tbh) = parsed.teams_base_host {
                                    payload.insert("teams_base_host".into(), json!(tbh));
                                }
                            }
                            Err(e) => return Ok(error_result(e)),
                        }
                    }
                    match self
                        .gateway(
                            reqwest::Method::POST,
                            "/bots",
                            &vexa_key,
                            Some(Value::Object(payload.clone())),
                            &[],
                        )
                        .await
                    {
                        Ok(v) => Ok(text_result(
                            serde_json::to_string_pretty(&v).unwrap_or_default(),
                        )),
                        // Idempotency case, matching main.py: a 409 means the meeting already
                        // exists for this key — look it up and return it as a soft success
                        // instead of surfacing a hard tool error, since `vexa.meeting_prep`
                        // documents `request_meeting_bot` as idempotent.
                        Err(e) if e.starts_with("HTTP 409") => {
                            let platform = payload
                                .get("platform")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let native_id = payload
                                .get("native_meeting_id")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            let meetings = self
                                .gateway(reqwest::Method::GET, "/meetings", &vexa_key, None, &[])
                                .await
                                .ok();
                            let found = match (&meetings, &platform, &native_id) {
                                (Some(Value::Array(list)), Some(pl), Some(nid)) => list
                                    .iter()
                                    .find(|m| {
                                        m.get("platform").and_then(Value::as_str)
                                            == Some(pl.as_str())
                                            && m.get("native_meeting_id").and_then(Value::as_str)
                                                == Some(nid.as_str())
                                    })
                                    .cloned(),
                                _ => None,
                            };
                            let body = match found {
                                Some(m) => json!({"status": "already_exists", "meeting": m}),
                                None => json!({"status": "already_exists", "detail": e}),
                            };
                            Ok(text_result(
                                serde_json::to_string_pretty(&body).unwrap_or_default(),
                            ))
                        }
                        Err(e) => Ok(error_result(e)),
                    }
                }
                "get_meeting_transcript" => {
                    let p: MeetingPlatformIdArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/transcripts/{}/{}", p.meeting_platform, p.meeting_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::GET, &path, &vexa_key, None, &[])
                            .await,
                    ))
                }
                "list_recordings" => {
                    let p: ListRecordingsArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let mut query = vec![
                        ("limit", p.limit.to_string()),
                        ("offset", p.offset.to_string()),
                    ];
                    if let Some(id) = p.meeting_db_id {
                        query.push(("meeting_id", id.to_string()));
                    }
                    Ok(to_result(
                        self.gateway(reqwest::Method::GET, "/recordings", &vexa_key, None, &query)
                            .await,
                    ))
                }
                "get_recording" => {
                    let p: RecordingIdArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/recordings/{}", p.recording_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::GET, &path, &vexa_key, None, &[])
                            .await,
                    ))
                }
                "delete_recording" => {
                    let p: RecordingIdArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/recordings/{}", p.recording_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                            .await,
                    ))
                }
                "get_recording_media_download" => {
                    let p: RecordingMediaDownloadArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!(
                        "/recordings/{}/media/{}/download",
                        p.recording_id, p.media_file_id
                    );
                    match self
                        .gateway(reqwest::Method::GET, &path, &vexa_key, None, &[])
                        .await
                    {
                        Ok(mut v) => {
                            self.rewrite_download_url(&mut v);
                            Ok(text_result(
                                serde_json::to_string_pretty(&v).unwrap_or_default(),
                            ))
                        }
                        Err(e) => Ok(error_result(e)),
                    }
                }
                "get_recording_config" => {
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    Ok(to_result(
                        self.gateway(
                            reqwest::Method::GET,
                            "/recording-config",
                            &vexa_key,
                            None,
                            &[],
                        )
                        .await,
                    ))
                }
                "update_recording_config" => {
                    let p: RecordingConfigUpdateArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    Ok(to_result(
                        self.gateway(
                            reqwest::Method::PUT,
                            "/recording-config",
                            &vexa_key,
                            Some(Value::Object(p.rest)),
                            &[],
                        )
                        .await,
                    ))
                }
                "get_meeting_bundle" => {
                    let p: MeetingBundleArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/transcripts/{}/{}", p.meeting_platform, p.meeting_id);
                    let mut result = match self
                        .gateway(reqwest::Method::GET, &path, &vexa_key, None, &[])
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => return Ok(error_result(e)),
                    };
                    if let Value::Object(ref mut obj) = result {
                        if !p.include_segments {
                            obj.remove("segments");
                        }
                        if !p.include_recordings {
                            obj.remove("recordings");
                        }
                    }
                    if p.include_media_download_urls {
                        if let Value::Object(ref mut obj) = result {
                            if let Some(Value::Array(recs)) = obj.get_mut("recordings") {
                                for rec in recs.iter_mut() {
                                    let Some(rid) = rec.get("id").and_then(Value::as_i64) else {
                                        continue;
                                    };
                                    let Some(Value::Array(mfs)) = rec.get_mut("media_files") else {
                                        continue;
                                    };
                                    for mf in mfs.iter_mut() {
                                        let Some(mf_id) = mf.get("id").and_then(Value::as_i64)
                                        else {
                                            continue;
                                        };
                                        let path =
                                            format!("/recordings/{rid}/media/{mf_id}/download");
                                        let outcome = match self
                                            .gateway(
                                                reqwest::Method::GET,
                                                &path,
                                                &vexa_key,
                                                None,
                                                &[],
                                            )
                                            .await
                                        {
                                            Ok(mut dl) => {
                                                self.rewrite_download_url(&mut dl);
                                                ("download", dl)
                                            }
                                            Err(e) => ("download_error", json!(e)),
                                        };
                                        if let Some(mf_obj) = mf.as_object_mut() {
                                            mf_obj.insert(outcome.0.to_string(), outcome.1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if p.include_share_link {
                        let mut query = vec![];
                        if let Some(ttl) = p.share_ttl_seconds {
                            query.push(("ttl_seconds", ttl.to_string()));
                        }
                        let share_path =
                            format!("/transcripts/{}/{}/share", p.meeting_platform, p.meeting_id);
                        match self
                            .gateway(reqwest::Method::POST, &share_path, &vexa_key, None, &query)
                            .await
                        {
                            Ok(share) => {
                                if let Value::Object(ref mut obj) = result {
                                    obj.insert("share_link".to_string(), share);
                                }
                            }
                            Err(e) => {
                                if let Value::Object(ref mut obj) = result {
                                    obj.insert("share_link_error".to_string(), json!(e));
                                }
                            }
                        }
                    }
                    Ok(text_result(
                        serde_json::to_string_pretty(&result).unwrap_or_default(),
                    ))
                }
                "create_transcript_share_link" => {
                    let p: ShareLinkArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let mut query = vec![];
                    if let Some(id) = p.meeting_db_id {
                        query.push(("meeting_id", id.to_string()));
                    }
                    if let Some(ttl) = p.ttl_seconds {
                        query.push(("ttl_seconds", ttl.to_string()));
                    }
                    let path =
                        format!("/transcripts/{}/{}/share", p.meeting_platform, p.meeting_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::POST, &path, &vexa_key, None, &query)
                            .await,
                    ))
                }
                "get_bot_status" => {
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    Ok(to_result(
                        self.gateway(reqwest::Method::GET, "/bots/status", &vexa_key, None, &[])
                            .await,
                    ))
                }
                "update_bot_config" => {
                    let p: UpdateBotConfigArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/bots/{}/{}/config", p.meeting_platform, p.meeting_id);
                    Ok(to_result(
                        self.gateway(
                            reqwest::Method::PUT,
                            &path,
                            &vexa_key,
                            Some(Value::Object(p.rest)),
                            &[],
                        )
                        .await,
                    ))
                }
                "stop_bot" => {
                    let p: MeetingPlatformIdArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/bots/{}/{}", p.meeting_platform, p.meeting_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                            .await,
                    ))
                }
                "list_meetings" => {
                    let p: ListMeetingsArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let mut query = vec![
                        ("limit", p.limit.to_string()),
                        ("offset", p.offset.to_string()),
                    ];
                    if let Some(s) = &p.status {
                        query.push(("status", s.clone()));
                    }
                    if let Some(pl) = &p.platform {
                        query.push(("platform", pl.clone()));
                    }
                    Ok(to_result(
                        self.gateway(reqwest::Method::GET, "/meetings", &vexa_key, None, &query)
                            .await,
                    ))
                }
                "update_meeting_data" => {
                    let p: UpdateMeetingDataArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/meetings/{}/{}", p.meeting_platform, p.meeting_id);
                    let payload = json!({"data": p.rest});
                    Ok(to_result(
                        self.gateway(reqwest::Method::PATCH, &path, &vexa_key, Some(payload), &[])
                            .await,
                    ))
                }
                "delete_meeting" => {
                    let p: MeetingPlatformIdArgs = parse_args(args)?;
                    let vexa_key = self.resolve_vexa_key(&token).await;
                    let path = format!("/meetings/{}/{}", p.meeting_platform, p.meeting_id);
                    Ok(to_result(
                        self.gateway(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                            .await,
                    ))
                }
                "search" => {
                    let p: SearchArgs = parse_args(args)?;
                    let limit = p.limit.unwrap_or(6).clamp(1, 20);
                    let query = p.query.trim().to_string();
                    if query.is_empty() {
                        return Ok(text_result("Query is empty"));
                    }
                    Ok(to_result(
                        self.hivemind(
                            reqwest::Method::POST,
                            "/knowledge/search",
                            &token,
                            Some(json!({"query": query, "limit": limit})),
                        )
                        .await,
                    ))
                }
                "meetings" => Ok(to_result(
                    self.hivemind(reqwest::Method::GET, "/meetings", &token, None)
                        .await,
                )),
                "transcript" => {
                    let p: MeetingIdArgs = parse_args(args)?;
                    let path = format!("/meetings/{}/transcript", p.meeting_id);
                    Ok(to_result(
                        self.hivemind(reqwest::Method::GET, &path, &token, None)
                            .await,
                    ))
                }
                "meeting_get" => {
                    let p: MeetingIdArgs = parse_args(args)?;
                    let path = format!("/meetings/{}", p.meeting_id);
                    Ok(to_result(
                        self.hivemind(reqwest::Method::GET, &path, &token, None)
                            .await,
                    ))
                }
                "documents" => Ok(to_result(
                    self.hivemind(reqwest::Method::GET, "/knowledge/documents", &token, None)
                        .await,
                )),
                "document_delete" => {
                    let p: DocumentIdArgs = parse_args(args)?;
                    let path = format!("/knowledge/documents/{}", p.document_id);
                    Ok(to_result(
                        self.hivemind(reqwest::Method::DELETE, &path, &token, None)
                            .await,
                    ))
                }
                "session" => {
                    let p: IngestSessionArgs = parse_args(args)?;
                    Ok(to_result(
                        self.hivemind(
                            reqwest::Method::POST,
                            "/knowledge/sessions",
                            &token,
                            Some(Value::Object(p.rest)),
                        )
                        .await,
                    ))
                }
                "meeting" => {
                    let p: IngestMeetingArgs = parse_args(args)?;
                    Ok(to_result(
                        self.hivemind(
                            reqwest::Method::POST,
                            "/meetings",
                            &token,
                            Some(Value::Object(p.rest)),
                        )
                        .await,
                    ))
                }
                _ => Err(ErrorData::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> KiokuMcpService {
        KiokuMcpService {
            http: reqwest::Client::new(),
            config: Arc::new(Config {
                kioku_api_url: "https://api.kioku.chat".to_string(),
                hivemind_api_url: String::new(),
                port: 0,
            }),
        }
    }

    #[test]
    fn rewrite_download_url_prefixes_relative_path() {
        let svc = service();
        let mut v = json!({"download_url": "/recordings/1/media/2/download/raw"});
        svc.rewrite_download_url(&mut v);
        assert_eq!(
            v["download_url"],
            "https://api.kioku.chat/recordings/1/media/2/download/raw"
        );
    }

    #[test]
    fn rewrite_download_url_swaps_minio_host() {
        let svc = service();
        let mut v = json!({"download_url": "http://minio:9000/vexa-recordings/foo.wav"});
        svc.rewrite_download_url(&mut v);
        assert_eq!(
            v["download_url"],
            "https://api.kioku.chat/vexa-recordings/foo.wav"
        );
    }

    #[test]
    fn rewrite_download_url_leaves_ordinary_url_untouched() {
        let svc = service();
        let mut v = json!({"download_url": "https://cdn.example.com/foo.wav"});
        svc.rewrite_download_url(&mut v);
        assert_eq!(v["download_url"], "https://cdn.example.com/foo.wav");
    }

    #[test]
    fn meeting_bundle_args_default_matches_python_defaults() {
        let args: MeetingBundleArgs = parse_args(None).unwrap();
        assert!(!args.include_segments);
        assert!(args.include_recordings);
        assert!(args.include_share_link);
        assert!(!args.include_media_download_urls);
    }
}
