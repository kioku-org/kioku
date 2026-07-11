use crate::app::KiokuMcpService;
use crate::auth::resolve_vexa_key;
use crate::error::{error_result, text_result, to_result};
use crate::tools::{parse_args, rewrite_download_url};
use rmcp::model::{CallToolResult, JsonObject};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::Value;

fn default_50() -> i64 {
    50
}

#[derive(Debug, Default, Deserialize)]
struct ListRecordingsArgs {
    #[serde(default = "default_50")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    meeting_db_id: Option<i64>,
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

pub(crate) async fn dispatch(
    svc: &KiokuMcpService,
    name: &str,
    args: Option<&JsonObject>,
    token: &str,
) -> Result<CallToolResult, ErrorData> {
    match name {
        "list_recordings" => {
            let p: ListRecordingsArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let mut query = vec![
                ("limit", p.limit.to_string()),
                ("offset", p.offset.to_string()),
            ];
            if let Some(id) = p.meeting_db_id {
                query.push(("meeting_id", id.to_string()));
            }
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::GET, "/recordings", &vexa_key, None, &query)
                    .await,
            ))
        }
        "get_recording" => {
            let p: RecordingIdArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/recordings/{}", p.recording_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::GET, &path, &vexa_key, None, &[])
                    .await,
            ))
        }
        "delete_recording" => {
            let p: RecordingIdArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/recordings/{}", p.recording_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                    .await,
            ))
        }
        "get_recording_media_download" => {
            let p: RecordingMediaDownloadArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!(
                "/recordings/{}/media/{}/download",
                p.recording_id, p.media_file_id
            );
            match svc
                .vexa
                .http_request(reqwest::Method::GET, &path, &vexa_key, None, &[])
                .await
            {
                Ok(mut v) => {
                    rewrite_download_url(&svc.config.kioku_api_url, &mut v);
                    Ok(text_result(
                        serde_json::to_string_pretty(&v).unwrap_or_default(),
                    ))
                }
                Err(e) => Ok(error_result(e)),
            }
        }
        "get_recording_config" => {
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            Ok(to_result(
                svc.vexa
                    .http_request(
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
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            Ok(to_result(
                svc.vexa
                    .http_request(
                        reqwest::Method::PUT,
                        "/recording-config",
                        &vexa_key,
                        Some(Value::Object(p.rest)),
                        &[],
                    )
                    .await,
            ))
        }
        _ => unreachable!("recordings::dispatch called with unrouted tool name {name}"),
    }
}
