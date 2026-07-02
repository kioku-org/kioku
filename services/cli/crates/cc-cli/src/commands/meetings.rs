use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;

/// Resolve a bot identifier (exact meeting id, or an unambiguous prefix of one)
/// against the running-bots list, returning (platform, native_meeting_id).
pub fn resolve_bot_kill_target(
    bots: &[cc_kioku::BotStatus],
    bot_id_or_prefix: &str,
) -> Result<(String, String)> {
    let matches: Vec<&cc_kioku::BotStatus> = bots
        .iter()
        .filter(|b| {
            b.native_meeting_id.as_deref() == Some(bot_id_or_prefix)
                || b.native_meeting_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(bot_id_or_prefix))
        })
        .collect();

    match matches.as_slice() {
        [only] => {
            let platform = only.platform.clone().ok_or_else(|| {
                anyhow::anyhow!("bot `{}` has no platform recorded", bot_id_or_prefix)
            })?;
            let native_meeting_id = only.native_meeting_id.clone().ok_or_else(|| {
                anyhow::anyhow!("bot `{}` has no meeting id recorded", bot_id_or_prefix)
            })?;
            Ok((platform, native_meeting_id))
        }
        [] => Err(anyhow::anyhow!(
            "bot `{}` not found — run `kioku meet` to inspect running bots",
            bot_id_or_prefix
        )),
        _ => Err(anyhow::anyhow!(
            "bot id `{}` is ambiguous — matches multiple running bots, use a longer prefix",
            bot_id_or_prefix
        )),
    }
}

pub async fn run(ctx: AppContext, link: Option<String>, kill: Option<String>) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if let Some(bot_id) = kill {
        let bots = client.list_bot_status().await?;
        let (platform, native_meeting_id) = resolve_bot_kill_target(&bots, &bot_id)?;
        client.stop_bot(&platform, &native_meeting_id).await?;
        println!("Stopped bot {bot_id}");
        return Ok(());
    }

    if let Some(target) = link.filter(|l| !l.eq_ignore_ascii_case("ls")) {
        let result = client.join_meeting(&target).await?;
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Joining {target}");
        }
        return Ok(());
    }

    let bots = client.list_bot_status().await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&bots)?);
        return Ok(());
    }
    if bots.is_empty() {
        println!("No running bots.");
    }
    for b in &bots {
        let platform = b.platform.as_deref().unwrap_or("?");
        let meeting_id = b.native_meeting_id.as_deref().unwrap_or("?");
        let status = b
            .normalized_status
            .as_deref()
            .or(b.status.as_deref())
            .unwrap_or("unknown");
        println!("{meeting_id} — {platform} ({status})");
    }
    Ok(())
}

pub async fn transcript(ctx: AppContext, meeting_id: String) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);
    let chunks = client.get_transcript(&meeting_id).await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&chunks)?);
        return Ok(());
    }
    if chunks.is_empty() {
        println!("No transcript.");
    }
    for c in &chunks {
        let speaker = c.get("speaker").and_then(|v| v.as_str()).unwrap_or("?");
        let text = c.get("text").and_then(|v| v.as_str()).unwrap_or("");
        println!("{speaker}: {text}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot_fixture(platform: &str, native_meeting_id: &str) -> cc_kioku::BotStatus {
        cc_kioku::BotStatus {
            platform: Some(platform.to_string()),
            native_meeting_id: Some(native_meeting_id.to_string()),
            status: Some("running".to_string()),
            normalized_status: Some("Up".to_string()),
            meeting_id_from_name: None,
        }
    }

    #[test]
    fn resolve_bot_kill_target_accepts_exact_id() {
        let bots = vec![bot_fixture("google_meet", "abc-defg-hij")];
        let (platform, id) = resolve_bot_kill_target(&bots, "abc-defg-hij").unwrap();
        assert_eq!(platform, "google_meet");
        assert_eq!(id, "abc-defg-hij");
    }

    #[test]
    fn resolve_bot_kill_target_accepts_unambiguous_prefix() {
        let bots = vec![bot_fixture("google_meet", "abc-defg-hij")];
        let (platform, id) = resolve_bot_kill_target(&bots, "abc-de").unwrap();
        assert_eq!(platform, "google_meet");
        assert_eq!(id, "abc-defg-hij");
    }

    #[test]
    fn resolve_bot_kill_target_errors_for_unknown_value() {
        let bots = vec![bot_fixture("google_meet", "abc-defg-hij")];
        let err = resolve_bot_kill_target(&bots, "zzz")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "bot `zzz` not found — run `kioku meet` to inspect running bots"
        );
    }

    #[test]
    fn resolve_bot_kill_target_errors_for_ambiguous_prefix() {
        let bots = vec![
            bot_fixture("google_meet", "abc-defg-hij"),
            bot_fixture("teams", "abc-1234567"),
        ];
        let err = resolve_bot_kill_target(&bots, "abc")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "bot id `abc` is ambiguous — matches multiple running bots, use a longer prefix"
        );
    }
}
