use anyhow::{Context, Result};
use cc_auth::{AuthFile, GoogleCalendarAuth};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use serde::Serialize;

use crate::config;
use crate::signin::{self, Provider};

/// Save Calendar tokens obtained as part of `kioku signin`'s OAuth round
/// trip (see `commands::auth::signin`) — `expires_at_secs` is Unix seconds
/// (NextAuth's `account.expires_at` convention); `GoogleCalendarAuth` wants
/// Unix ms.
pub fn save_from_signin(
    access_token: String,
    refresh_token: String,
    expires_at_secs: Option<i64>,
) -> Result<()> {
    GoogleCalendarAuth {
        access_token,
        refresh_token,
        expires_at: expires_at_secs
            .map(|secs| secs * 1000)
            .unwrap_or_else(|| cc_auth::now_ms() + 3_600_000),
    }
    .save()
}

// ─── Connect / reconnect ────────────────────────────────────────────────────
//
// Runs the same dashboard-mediated OAuth round trip `kioku signin` uses
// (signin::run_oauth_flow with Provider::Google always also requests
// Calendar scope) — there's no separate Google client embedded in the CLI
// for this; the dashboard holds the confidential client secret and the CLI
// only ever sees the resulting access/refresh tokens.

async fn connect(dashboard_url: &str) -> Result<()> {
    println!("Kioku needs read-only Google Calendar access to show your upcoming meetings.");
    println!("It will not edit, create, or delete events.");
    let result = signin::run_oauth_flow(dashboard_url, &Provider::Google).await?;
    let access_token = result
        .google_access_token
        .ok_or_else(|| anyhow::anyhow!("Google Calendar access wasn't granted"))?;
    let refresh_token = result.google_refresh_token.ok_or_else(|| {
        anyhow::anyhow!(
            "Google didn't return a refresh token — if you've granted Kioku access before, \
             revoke it at https://myaccount.google.com/permissions and try again"
        )
    })?;
    save_from_signin(access_token, refresh_token, result.google_token_expires_at)
}

// ─── Token refresh ──────────────────────────────────────────────────────────
//
// Refreshing also goes through the dashboard (POST /api/cli/google-calendar/
// refresh, authenticated with the caller's normal Kioku bearer token) rather
// than calling Google directly — the refresh token was issued to the
// dashboard's confidential Google client, so redeeming it needs that
// client's secret, which the CLI must never hold.

#[derive(serde::Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: i64,
}

async fn refresh_token(
    dashboard_url: &str,
    auth: &AuthFile,
    cal: &GoogleCalendarAuth,
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{dashboard_url}/api/cli/google-calendar/refresh"))
        .bearer_auth(&auth.token)
        .json(&serde_json::json!({ "refresh_token": cal.refresh_token }))
        .send()
        .await
        .context("token refresh request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("token refresh failed");
    }

    let token: RefreshResponse = resp.json().await.context("invalid refresh response")?;

    GoogleCalendarAuth {
        access_token: token.access_token.clone(),
        refresh_token: cal.refresh_token.clone(),
        expires_at: cc_auth::now_ms() + token.expires_in * 1000,
    }
    .save()?;

    Ok(token.access_token)
}

/// Returns a valid Calendar access token, transparently connecting or
/// reconnecting Google Calendar access if there's no usable token yet:
///
///   - no token saved            → run the consent flow, then use the new token
///   - token valid                → use it
///   - token expired              → refresh silently
///   - refresh fails (revoked)    → run the consent flow again
pub async fn ensure_valid_token_or_connect(auth: &AuthFile) -> Result<String> {
    let dashboard_url = config::resolve_dashboard_url(&auth.server_url);

    if let Some(cal) = GoogleCalendarAuth::load()? {
        if !cal.is_expired() {
            return Ok(cal.access_token);
        }
        if let Ok(token) = refresh_token(&dashboard_url, auth, &cal).await {
            return Ok(token);
        }
        // Refresh failed (e.g. access revoked, or a leftover token from
        // before this flow existed) — fall through and reconnect.
    }

    connect(&dashboard_url).await?;

    let cal = GoogleCalendarAuth::load()?
        .ok_or_else(|| anyhow::anyhow!("calendar sign-in completed but no token was saved"))?;
    Ok(cal.access_token)
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
