use crate::app::KiokuMcpService;
use crate::auth::resolve_vexa_key;
use crate::error::{error_result, text_result, to_result};
use crate::tools::parser::parse_meeting_url;
use crate::tools::{parse_args, rewrite_download_url};
use rmcp::model::{CallToolResult, JsonObject};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{json, Value};

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

fn default_true() -> bool {
    true
}

fn default_20() -> i64 {
    20
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

#[derive(Debug, Default, Deserialize)]
struct UpdateMeetingDataArgs {
    meeting_id: String,
    #[serde(default = "default_platform")]
    meeting_platform: String,
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
        "request_meeting_bot" => {
            let p: RequestMeetingBotArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let mut payload = p.rest;
            if let Some(Value::String(url)) = payload.remove("meeting_url") {
                match parse_meeting_url(&url) {
                    Ok(parsed) => {
                        payload.insert("platform".into(), json!(parsed.platform));
                        payload.insert("native_meeting_id".into(), json!(parsed.native_meeting_id));
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
            match svc
                .vexa
                .http_request(
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
                    let meetings = svc
                        .vexa
                        .http_request(reqwest::Method::GET, "/meetings", &vexa_key, None, &[])
                        .await
                        .ok();
                    let found = match (&meetings, &platform, &native_id) {
                        (Some(Value::Array(list)), Some(pl), Some(nid)) => list
                            .iter()
                            .find(|m| {
                                m.get("platform").and_then(Value::as_str) == Some(pl.as_str())
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
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/transcripts/{}/{}", p.meeting_platform, p.meeting_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::GET, &path, &vexa_key, None, &[])
                    .await,
            ))
        }
        "get_meeting_bundle" => {
            let p: MeetingBundleArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/transcripts/{}/{}", p.meeting_platform, p.meeting_id);
            let mut result = match svc
                .vexa
                .http_request(reqwest::Method::GET, &path, &vexa_key, None, &[])
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
                                let Some(mf_id) = mf.get("id").and_then(Value::as_i64) else {
                                    continue;
                                };
                                let path = format!("/recordings/{rid}/media/{mf_id}/download");
                                let outcome = match svc
                                    .vexa
                                    .http_request(reqwest::Method::GET, &path, &vexa_key, None, &[])
                                    .await
                                {
                                    Ok(mut dl) => {
                                        rewrite_download_url(&svc.config.kioku_api_url, &mut dl);
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
                match svc
                    .vexa
                    .http_request(reqwest::Method::POST, &share_path, &vexa_key, None, &query)
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
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let mut query = vec![];
            if let Some(id) = p.meeting_db_id {
                query.push(("meeting_id", id.to_string()));
            }
            if let Some(ttl) = p.ttl_seconds {
                query.push(("ttl_seconds", ttl.to_string()));
            }
            let path = format!("/transcripts/{}/{}/share", p.meeting_platform, p.meeting_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::POST, &path, &vexa_key, None, &query)
                    .await,
            ))
        }
        "get_bot_status" => {
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::GET, "/bots/status", &vexa_key, None, &[])
                    .await,
            ))
        }
        "update_bot_config" => {
            let p: UpdateBotConfigArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/bots/{}/{}/config", p.meeting_platform, p.meeting_id);
            Ok(to_result(
                svc.vexa
                    .http_request(
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
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/bots/{}/{}", p.meeting_platform, p.meeting_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                    .await,
            ))
        }
        "list_meetings" => {
            let p: ListMeetingsArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
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
                svc.vexa
                    .http_request(reqwest::Method::GET, "/meetings", &vexa_key, None, &query)
                    .await,
            ))
        }
        "update_meeting_data" => {
            let p: UpdateMeetingDataArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/meetings/{}/{}", p.meeting_platform, p.meeting_id);
            let payload = json!({"data": p.rest});
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::PATCH, &path, &vexa_key, Some(payload), &[])
                    .await,
            ))
        }
        "delete_meeting" => {
            let p: MeetingPlatformIdArgs = parse_args(args)?;
            let vexa_key = resolve_vexa_key(&svc.http, &svc.config.hivemind_api_url, token).await;
            let path = format!("/meetings/{}/{}", p.meeting_platform, p.meeting_id);
            Ok(to_result(
                svc.vexa
                    .http_request(reqwest::Method::DELETE, &path, &vexa_key, None, &[])
                    .await,
            ))
        }
        _ => unreachable!("meetings::dispatch called with unrouted tool name {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::parse_args;

    #[test]
    fn meeting_bundle_args_default_matches_python_defaults() {
        let args: MeetingBundleArgs = parse_args(None).unwrap();
        assert!(!args.include_segments);
        assert!(args.include_recordings);
        assert!(args.include_share_link);
        assert!(!args.include_media_download_urls);
    }
}
