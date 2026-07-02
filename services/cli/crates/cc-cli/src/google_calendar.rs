use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use cc_auth::GoogleCalendarAuth;
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

fn client_id() -> Result<String> {
    std::env::var("KIOKU_GOOGLE_CALENDAR_CLIENT_ID").context(
        "KIOKU_GOOGLE_CALENDAR_CLIENT_ID is not set. `kioku cal` needs its own Google OAuth \
         client (Desktop app type, calendar.readonly scope only) — separate from the main \
         dashboard sign-in. Set KIOKU_GOOGLE_CALENDAR_CLIENT_ID (and, if your client has one, \
         KIOKU_GOOGLE_CALENDAR_CLIENT_SECRET) once that's created.",
    )
}

fn client_secret() -> Option<String> {
    std::env::var("KIOKU_GOOGLE_CALENDAR_CLIENT_SECRET").ok()
}

fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

// ─── Sign-in flow (direct CLI <-> Google, PKCE, loopback redirect) ─────────

pub async fn signin_calendar() -> Result<()> {
    let client_id = client_id()?;
    let client_secret = client_secret();
    let (verifier, challenge) = generate_pkce();
    let state = uuid::Uuid::new_v4().to_string();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let auth_url = format!(
        "{GOOGLE_AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(CALENDAR_SCOPE),
        challenge,
        state,
    );

    println!("Opening browser to grant kioku read-only access to your Google Calendar...");
    if webbrowser::open(&auth_url).is_err() {
        println!("Couldn't open a browser automatically. Open this URL:");
        println!("{auth_url}");
    }

    let code = wait_for_auth_code(listener, &state).await?;

    let client = reqwest::Client::new();
    let mut params = vec![
        ("client_id", client_id.as_str()),
        ("code", code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];
    if let Some(secret) = client_secret.as_deref() {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .context("token exchange request failed")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Google token exchange failed: {body}");
    }

    let token: TokenResponse = resp.json().await.context("invalid token response")?;
    let refresh_token = token.refresh_token.ok_or_else(|| {
        anyhow::anyhow!(
            "Google did not return a refresh token. If you've granted kioku access before, \
             revoke it at https://myaccount.google.com/permissions and try again — Google only \
             issues a refresh token on the first consent for a given client."
        )
    })?;

    GoogleCalendarAuth {
        access_token: token.access_token,
        refresh_token,
        expires_at: cc_auth::now_ms() + token.expires_in * 1000,
    }
    .save()?;

    println!("Google Calendar access granted.");
    Ok(())
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

async fn wait_for_auth_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    use tokio::time::{timeout, Duration as TokioDuration};

    let (mut stream, _) = timeout(TokioDuration::from_secs(120), listener.accept())
        .await
        .map_err(|_| anyhow::anyhow!("Timed out after 2 minutes — browser sign-in not completed"))?
        .map_err(|e| anyhow::anyhow!("Socket error: {}", e))?;

    let mut buf = vec![0u8; 16_384];
    let n = timeout(TokioDuration::from_secs(5), stream.read(&mut buf))
        .await
        .map_err(|_| anyhow::anyhow!("Timed out reading callback request"))?
        .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

    let raw = std::str::from_utf8(&buf[..n]).unwrap_or("");
    let path = raw
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP request in callback"))?;

    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params: HashMap<&str, &str> = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .collect();

    let state = params.get("state").copied().unwrap_or("");
    if state != expected_state {
        let _ = respond(
            &mut stream,
            400,
            "State mismatch — possible CSRF. Please retry.",
        )
        .await;
        anyhow::bail!("State mismatch — possible CSRF. Please try again.");
    }

    if let Some(err) = params.get("error") {
        let _ = respond(
            &mut stream,
            400,
            "Google reported an error — check your terminal.",
        )
        .await;
        anyhow::bail!("Google OAuth error: {err}");
    }

    let code = params
        .get("code")
        .map(|v| urlencoding::decode(v).unwrap_or_default().into_owned())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| anyhow::anyhow!("No authorization code in callback"))?;

    respond(
        &mut stream,
        200,
        "Google Calendar access granted. You can close this tab.",
    )
    .await?;

    Ok(code)
}

async fn respond(stream: &mut tokio::net::TcpStream, status: u16, message: &str) -> Result<()> {
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let body = format!(
        "<!DOCTYPE html><html><body style=\"font-family:system-ui;text-align:center;padding-top:4rem\"><h2>{message}</h2></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

// ─── Token refresh ──────────────────────────────────────────────────────────

/// Returns a valid Calendar access token, transparently connecting or
/// reconnecting Google Calendar access if there's no usable token yet:
///
///   - no token saved            → run the consent flow, then use the new token
///   - token valid                → use it
///   - token expired              → refresh silently
///   - refresh fails (revoked)    → run the consent flow again
pub async fn ensure_valid_token_or_connect() -> Result<String> {
    if let Some(auth) = GoogleCalendarAuth::load()? {
        if !auth.is_expired() {
            return Ok(auth.access_token);
        }
        if let Ok(token) = refresh_token(&auth).await {
            return Ok(token);
        }
        // Refresh failed (e.g. access revoked) — fall through and reconnect.
    }

    println!("Kioku needs read-only Google Calendar access to show your upcoming meetings.");
    println!("It will not edit, create, or delete events.");
    signin_calendar().await?;

    let auth = GoogleCalendarAuth::load()?
        .ok_or_else(|| anyhow::anyhow!("calendar sign-in completed but no token was saved"))?;
    Ok(auth.access_token)
}

async fn refresh_token(auth: &GoogleCalendarAuth) -> Result<String> {
    let client_id = client_id()?;
    let client_secret = client_secret();

    let client = reqwest::Client::new();
    let mut params = vec![
        ("client_id", client_id.as_str()),
        ("refresh_token", auth.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = client_secret.as_deref() {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .context("token refresh request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("token refresh failed");
    }

    let token: TokenResponse = resp.json().await.context("invalid refresh response")?;

    GoogleCalendarAuth {
        access_token: token.access_token.clone(),
        refresh_token: auth.refresh_token.clone(),
        expires_at: cc_auth::now_ms() + token.expires_in * 1000,
    }
    .save()?;

    Ok(token.access_token)
}

// ─── Calendar API ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub summary: String,
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

pub async fn list_events(
    access_token: &str,
    time_min: DateTime<Local>,
    time_max: DateTime<Local>,
) -> Result<Vec<CalendarEvent>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(access_token)
        .query(&[
            ("timeMin", time_min.to_rfc3339()),
            ("timeMax", time_max.to_rfc3339()),
            ("singleEvents", "true".to_string()),
            ("orderBy", "startTime".to_string()),
        ])
        .send()
        .await
        .context("calendar list request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Google Calendar API error ({status}): {body}");
    }

    let raw: serde_json::Value = resp.json().await.context("invalid calendar response")?;
    let items = raw
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let events = items
        .into_iter()
        .map(|item| {
            let summary = item
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("(no title)")
                .to_string();
            let start = item
                .get("start")
                .and_then(|s| s.get("dateTime").or_else(|| s.get("date")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let link = item
                .get("hangoutLink")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    item.get("conferenceData")
                        .and_then(|c| c.get("entryPoints"))
                        .and_then(|e| e.as_array())
                        .and_then(|arr| {
                            arr.iter().find(|ep| {
                                ep.get("entryPointType").and_then(|t| t.as_str()) == Some("video")
                            })
                        })
                        .and_then(|ep| ep.get("uri"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .or_else(|| {
                    item.get("htmlLink")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                });
            CalendarEvent {
                summary,
                start,
                link,
            }
        })
        .collect();

    Ok(events)
}

// ─── Date range helpers ─────────────────────────────────────────────────────

pub fn range_today() -> (DateTime<Local>, DateTime<Local>) {
    let start = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid");
    let start = Local
        .from_local_datetime(&start)
        .single()
        .unwrap_or_else(Local::now);
    (start, start + Duration::days(1))
}

pub fn range_week() -> (DateTime<Local>, DateTime<Local>) {
    let (start, _) = range_today();
    (start, start + Duration::days(7))
}

pub fn range_for_date(date: NaiveDate) -> (DateTime<Local>, DateTime<Local>) {
    let start = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
    let start = Local
        .from_local_datetime(&start)
        .single()
        .unwrap_or_else(Local::now);
    (start, start + Duration::days(1))
}

/// Strictly parses `dd/mm/yyyy` — rejects ambiguous or out-of-range values
/// (e.g. a month > 12) rather than silently misinterpreting them as mm/dd.
pub fn parse_date_ddmmyyyy(s: &str) -> Result<NaiveDate> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 3 {
        anyhow::bail!("invalid date `{s}` — expected dd/mm/yyyy");
    }
    let day: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid day in `{s}` — expected dd/mm/yyyy"))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid month in `{s}` — expected dd/mm/yyyy"))?;
    let year: i32 = parts[2]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid year in `{s}` — expected dd/mm/yyyy"))?;

    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("`{s}` is not a valid date — expected dd/mm/yyyy"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_ddmmyyyy_accepts_valid_date() {
        let d = parse_date_ddmmyyyy("25/12/2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 12, 25).unwrap());
    }

    #[test]
    fn parse_date_ddmmyyyy_rejects_month_over_12() {
        // Would silently succeed as mm/dd if parsed loosely (month=25 invalid either way,
        // but the real risk case is e.g. 13/01/2026 vs treating 01 as month in mm/dd).
        let err = parse_date_ddmmyyyy("25/13/2026").unwrap_err().to_string();
        assert!(err.contains("not a valid date"), "{err}");
    }

    #[test]
    fn parse_date_ddmmyyyy_rejects_wrong_format() {
        assert!(parse_date_ddmmyyyy("2026-12-25").is_err());
        assert!(parse_date_ddmmyyyy("25/12").is_err());
        assert!(parse_date_ddmmyyyy("abc/12/2026").is_err());
    }

    #[test]
    fn range_week_is_seven_days_after_today() {
        let (today_start, _) = range_today();
        let (week_start, week_end) = range_week();
        assert_eq!(today_start, week_start);
        assert_eq!(week_end - week_start, Duration::days(7));
    }
}
