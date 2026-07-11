use crate::app::KiokuMcpService;
use crate::error::{text_result, to_result};
use crate::tools::parse_args;
use rmcp::model::{CallToolResult, JsonObject};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::Value;

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

pub(crate) async fn dispatch(
    svc: &KiokuMcpService,
    name: &str,
    args: Option<&JsonObject>,
    token: &str,
) -> Result<CallToolResult, ErrorData> {
    match name {
        "search" => {
            let p: SearchArgs = parse_args(args)?;
            let limit = p.limit.unwrap_or(6).clamp(1, 20);
            let query = p.query.trim().to_string();
            if query.is_empty() {
                return Ok(text_result("Query is empty"));
            }
            Ok(to_result(
                svc.hivemind
                    .http_request(
                        reqwest::Method::POST,
                        "/knowledge/search",
                        token,
                        Some(serde_json::json!({"query": query, "limit": limit})),
                    )
                    .await,
            ))
        }
        "meetings" => Ok(to_result(
            svc.hivemind
                .http_request(reqwest::Method::GET, "/meetings", token, None)
                .await,
        )),
        "transcript" => {
            let p: MeetingIdArgs = parse_args(args)?;
            let path = format!("/meetings/{}/transcript", p.meeting_id);
            Ok(to_result(
                svc.hivemind
                    .http_request(reqwest::Method::GET, &path, token, None)
                    .await,
            ))
        }
        "meeting_get" => {
            let p: MeetingIdArgs = parse_args(args)?;
            let path = format!("/meetings/{}", p.meeting_id);
            Ok(to_result(
                svc.hivemind
                    .http_request(reqwest::Method::GET, &path, token, None)
                    .await,
            ))
        }
        "documents" => Ok(to_result(
            svc.hivemind
                .http_request(reqwest::Method::GET, "/knowledge/documents", token, None)
                .await,
        )),
        "document_delete" => {
            let p: DocumentIdArgs = parse_args(args)?;
            let path = format!("/knowledge/documents/{}", p.document_id);
            Ok(to_result(
                svc.hivemind
                    .http_request(reqwest::Method::DELETE, &path, token, None)
                    .await,
            ))
        }
        "session" => {
            let p: IngestSessionArgs = parse_args(args)?;
            Ok(to_result(
                svc.hivemind
                    .http_request(
                        reqwest::Method::POST,
                        "/knowledge/sessions",
                        token,
                        Some(Value::Object(p.rest)),
                    )
                    .await,
            ))
        }
        "meeting" => {
            let p: IngestMeetingArgs = parse_args(args)?;
            Ok(to_result(
                svc.hivemind
                    .http_request(
                        reqwest::Method::POST,
                        "/meetings",
                        token,
                        Some(Value::Object(p.rest)),
                    )
                    .await,
            ))
        }
        _ => unreachable!("knowledge::dispatch called with unrouted tool name {name}"),
    }
}
