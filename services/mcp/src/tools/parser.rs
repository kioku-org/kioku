use crate::error::{error_result, text_result};
use crate::tools::parse_args;
use rmcp::model::{CallToolResult, JsonObject};
use rmcp::ErrorData;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Debug, Serialize, PartialEq)]
pub struct ParseMeetingLinkResponse {
    pub platform: String,
    pub native_meeting_id: String,
    pub passcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams_base_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_url: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ParseMeetingLinkArgs {
    meeting_url: String,
}

pub(crate) fn parse_meeting_link_tool(
    args: Option<&JsonObject>,
) -> Result<CallToolResult, ErrorData> {
    let p: ParseMeetingLinkArgs = parse_args(args)?;
    Ok(match parse_meeting_url(&p.meeting_url) {
        Ok(r) => text_result(serde_json::to_string_pretty(&r).unwrap_or_default()),
        Err(e) => error_result(e),
    })
}

fn is_teams_enterprise_host(host: &str) -> bool {
    const ENTERPRISE_HOSTS: &[&str] = &["teams.microsoft.com"];
    ENTERPRISE_HOSTS.contains(&host)
        || host.ends_with(".teams.microsoft.us")
        || host.ends_with(".teams.microsoft.com")
}

fn query_param<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

pub fn parse_meeting_url(meeting_url: &str) -> Result<ParseMeetingLinkResponse, String> {
    let url = meeting_url.trim();
    if url.is_empty() {
        return Err("meeting_url cannot be empty".to_string());
    }
    let parsed = Url::parse(url).map_err(|_| "meeting_url is not a well-formed URL".to_string())?;
    let host = parsed.host_str().unwrap_or("").to_lowercase();
    let path = parsed.path().to_string();
    let query: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut warnings: Vec<String> = vec![];

    // Google Meet
    if host == "meet.google.com" {
        if path.starts_with("/lookup/") {
            return Err("Google Meet /lookup/ URLs cannot be joined directly. Use the standard meeting link from your calendar invite.".to_string());
        }
        let code = path
            .trim_matches('/')
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let standard = regex::Regex::new(r"^[a-z]{3}-[a-z]{4}-[a-z]{3}$").unwrap();
        let nickname = regex::Regex::new(r"^[a-z0-9][a-z0-9-]{3,38}[a-z0-9]$").unwrap();
        if standard.is_match(&code) {
            return Ok(ParseMeetingLinkResponse {
                platform: "google_meet".to_string(),
                native_meeting_id: code,
                passcode: None,
                teams_base_host: None,
                meeting_url: None,
                warnings,
            });
        }
        if nickname.is_match(&code) {
            warnings.push("Custom Google Meet nickname URL detected. This works for Google Workspace accounts only.".to_string());
            return Ok(ParseMeetingLinkResponse {
                platform: "google_meet".to_string(),
                native_meeting_id: code,
                passcode: None,
                teams_base_host: None,
                meeting_url: None,
                warnings,
            });
        }
        return Err("Invalid Google Meet URL: expected https://meet.google.com/abc-defg-hij or a custom Workspace nickname.".to_string());
    }

    // Teams personal
    if host.ends_with("teams.live.com") {
        let re = regex::Regex::new(r"^/meet/(\d{10,15})/?$").unwrap();
        let Some(caps) = re.captures(&path) else {
            return Err(
                "Unsupported teams.live.com URL format. Expected /meet/<10-15 digit id>."
                    .to_string(),
            );
        };
        let native_id = caps[1].to_string();
        let passcode = query_param(&query, "p").map(str::to_string);
        if passcode.is_none() {
            warnings.push(
                "Teams meeting link has no ?p= passcode. Many Teams meetings require it."
                    .to_string(),
            );
        }
        return Ok(ParseMeetingLinkResponse {
            platform: "teams".to_string(),
            native_meeting_id: native_id,
            passcode,
            teams_base_host: None,
            meeting_url: None,
            warnings,
        });
    }

    // Teams enterprise
    if is_teams_enterprise_host(&host) {
        let fragment = parsed.fragment().unwrap_or("");
        let re = regex::Regex::new(r"^/meet/(\d{10,15})/?$").unwrap();

        if (path.trim_end_matches('/') == "/v2" || path.trim_end_matches('/').is_empty())
            && fragment.starts_with("/meet/")
        {
            if let Ok(frag_url) = Url::parse(&format!("https://x{fragment}")) {
                if let Some(caps) = re.captures(frag_url.path()) {
                    let native_id = caps[1].to_string();
                    let frag_query: Vec<(String, String)> = frag_url
                        .query_pairs()
                        .map(|(k, v)| (k.into_owned(), v.into_owned()))
                        .collect();
                    let passcode = query_param(&frag_query, "p").map(str::to_string);
                    if passcode.is_none() {
                        warnings.push("Teams meeting link has no ?p= passcode. Many Teams meetings require it.".to_string());
                    }
                    return Ok(ParseMeetingLinkResponse {
                        platform: "teams".to_string(),
                        native_meeting_id: native_id,
                        passcode,
                        teams_base_host: Some(host),
                        meeting_url: None,
                        warnings,
                    });
                }
            }
        }

        if let Some(caps) = re.captures(&path) {
            let native_id = caps[1].to_string();
            let passcode = query_param(&query, "p").map(str::to_string);
            if passcode.is_none() {
                warnings.push(
                    "Teams meeting link has no ?p= passcode. Many Teams meetings require it."
                        .to_string(),
                );
            }
            return Ok(ParseMeetingLinkResponse {
                platform: "teams".to_string(),
                native_meeting_id: native_id,
                passcode,
                teams_base_host: Some(host),
                meeting_url: None,
                warnings,
            });
        }

        if path.contains("/l/meetup-join/") {
            let mut hasher = Sha256::new();
            hasher.update(url.as_bytes());
            let url_hash = hex::encode(hasher.finalize())[..16].to_string();
            warnings.push(
                "Legacy Teams enterprise URL detected. The full URL will be passed directly to the bot. \
                 The meeting ID shown is a stable hash of the URL used for deduplication."
                    .to_string(),
            );
            return Ok(ParseMeetingLinkResponse {
                platform: "teams".to_string(),
                native_meeting_id: url_hash,
                passcode: None,
                teams_base_host: None,
                meeting_url: Some(url.to_string()),
                warnings,
            });
        }

        return Err("Unsupported Teams enterprise URL format. Expected /meet/<id>?p=<passcode> or /l/meetup-join/...".to_string());
    }

    // Zoom Events
    if host == "events.zoom.us" || host == "ev.zoom.com" || host.ends_with(".events.zoom.us") {
        return Err("Zoom Events links are not supported. Attendees receive unique per-registrant join links via email; these cannot be shared with a bot.".to_string());
    }

    // Zoom
    if host.contains("zoom.us") || host.contains("zoomgov.com") {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let native_id = if parts.len() >= 2 && (parts[0] == "j" || parts[0] == "w") {
            parts[1].to_string()
        } else if parts.len() >= 3 && parts[0] == "wc" && parts[1] == "join" {
            parts[2].to_string()
        } else if parts.len() >= 2 && parts[0] == "my" {
            return Err("Zoom personal meeting room links (/my/...) are not supported. Ask the host to share a direct meeting link (/j/<id>).".to_string());
        } else {
            String::new()
        };
        let digits_only = regex::Regex::new(r"^\d{9,11}$").unwrap();
        if !digits_only.is_match(&native_id) {
            return Err(
                "Unsupported Zoom URL format. Expected https://zoom.us/j/<9-11 digit id>."
                    .to_string(),
            );
        }
        let passcode = query_param(&query, "pwd").map(str::to_string);
        return Ok(ParseMeetingLinkResponse {
            platform: "zoom".to_string(),
            native_meeting_id: native_id,
            passcode,
            teams_base_host: None,
            meeting_url: None,
            warnings,
        });
    }

    Err("Unsupported meeting URL (unknown provider).".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_meet_standard_code() {
        let r = parse_meeting_url("https://meet.google.com/abc-defg-hij").unwrap();
        assert_eq!(r.platform, "google_meet");
        assert_eq!(r.native_meeting_id, "abc-defg-hij");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn google_meet_lookup_rejected() {
        assert!(parse_meeting_url("https://meet.google.com/lookup/abc123").is_err());
    }

    #[test]
    fn zoom_j_link() {
        let r = parse_meeting_url("https://zoom.us/j/1234567890?pwd=secret").unwrap();
        assert_eq!(r.platform, "zoom");
        assert_eq!(r.native_meeting_id, "1234567890");
        assert_eq!(r.passcode.as_deref(), Some("secret"));
    }

    #[test]
    fn zoom_personal_room_rejected() {
        assert!(parse_meeting_url("https://zoom.us/my/someone").is_err());
    }

    #[test]
    fn teams_live_personal() {
        let r = parse_meeting_url("https://teams.live.com/meet/1234567890123?p=abc").unwrap();
        assert_eq!(r.platform, "teams");
        assert_eq!(r.native_meeting_id, "1234567890123");
        assert_eq!(r.passcode.as_deref(), Some("abc"));
    }

    #[test]
    fn teams_short_new_style() {
        let r = parse_meeting_url("https://teams.microsoft.com/meet/1234567890123?p=xyz").unwrap();
        assert_eq!(r.platform, "teams");
        assert_eq!(r.native_meeting_id, "1234567890123");
        assert_eq!(r.teams_base_host.as_deref(), Some("teams.microsoft.com"));
    }

    #[test]
    fn unsupported_host_rejected() {
        assert!(parse_meeting_url("https://example.com/meeting/123").is_err());
    }

    #[test]
    fn empty_url_rejected() {
        assert!(parse_meeting_url("").is_err());
    }
}
