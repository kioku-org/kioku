mod knowledge;
mod meetings;
pub(crate) mod parser;
pub(crate) mod prompts;
mod recordings;

use crate::app::KiokuMcpService;
use crate::error::schema;
use rmcp::model::{CallToolResult, JsonObject, Tool};
use rmcp::ErrorData;
use serde_json::{json, Value};

pub(crate) fn parse_args<T: serde::de::DeserializeOwned + Default>(
    args: Option<&JsonObject>,
) -> Result<T, ErrorData> {
    match args {
        None => Ok(T::default()),
        Some(obj) => serde_json::from_value(Value::Object(obj.clone()))
            .map_err(|e| ErrorData::invalid_params(format!("Invalid arguments: {e}"), None)),
    }
}

pub(crate) fn rewrite_download_url(kioku_api_url: &str, data: &mut Value) {
    let Some(dl) = data
        .get("download_url")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    if dl.starts_with('/') {
        data["download_url"] = json!(format!("{kioku_api_url}{dl}"));
    } else if dl.contains("minio:") || dl.contains("minio/") {
        if let (Ok(base), Ok(parsed)) = (url::Url::parse(kioku_api_url), url::Url::parse(&dl)) {
            let mut rewritten = base;
            rewritten.set_path(parsed.path());
            data["download_url"] = json!(rewritten.to_string());
        }
    }
}

pub(crate) async fn dispatch(
    svc: &KiokuMcpService,
    name: &str,
    args: Option<&JsonObject>,
    token: &str,
) -> Result<CallToolResult, ErrorData> {
    match name {
        "parse_meeting_link" => parser::parse_meeting_link_tool(args),
        "search" | "meetings" | "transcript" | "meeting_get" | "documents" | "document_delete"
        | "session" | "meeting" => knowledge::dispatch(svc, name, args, token).await,
        "request_meeting_bot"
        | "get_meeting_transcript"
        | "get_meeting_bundle"
        | "create_transcript_share_link"
        | "get_bot_status"
        | "update_bot_config"
        | "stop_bot"
        | "list_meetings"
        | "update_meeting_data"
        | "delete_meeting" => meetings::dispatch(svc, name, args, token).await,
        "list_recordings"
        | "get_recording"
        | "delete_recording"
        | "get_recording_media_download"
        | "get_recording_config"
        | "update_recording_config" => recordings::dispatch(svc, name, args, token).await,
        _ => Err(ErrorData::method_not_found::<
            rmcp::model::CallToolRequestMethod,
        >()),
    }
}

pub(crate) fn all_tools() -> Vec<Tool> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_download_url_prefixes_relative_path() {
        let mut v = json!({"download_url": "/recordings/1/media/2/download/raw"});
        rewrite_download_url("https://api.kioku.chat", &mut v);
        assert_eq!(
            v["download_url"],
            "https://api.kioku.chat/recordings/1/media/2/download/raw"
        );
    }

    #[test]
    fn rewrite_download_url_swaps_minio_host() {
        let mut v = json!({"download_url": "http://minio:9000/vexa-recordings/foo.wav"});
        rewrite_download_url("https://api.kioku.chat", &mut v);
        assert_eq!(
            v["download_url"],
            "https://api.kioku.chat/vexa-recordings/foo.wav"
        );
    }

    #[test]
    fn rewrite_download_url_leaves_ordinary_url_untouched() {
        let mut v = json!({"download_url": "https://cdn.example.com/foo.wav"});
        rewrite_download_url("https://api.kioku.chat", &mut v);
        assert_eq!(v["download_url"], "https://cdn.example.com/foo.wav");
    }
}
