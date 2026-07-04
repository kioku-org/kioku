//! Replaces the standalone `router` service. That service special-cased `POST /bots` /
//! `DELETE /bots/{platform}/{id}` with a sticky, in-memory local/RunPod split — but nothing
//! ever called those routes (runtime-api only exposes `/containers`, not `/bots`), so real
//! traffic went through its catch-all proxy instead, which routed on a single static
//! `USE_LOCAL_RESOURCE` env var only. There was no actual per-bot load balancing happening.
//!
//! This does what the router's dead code was meant to do, for real: pick a backend once per
//! meeting at spawn time based on current local-bot occupancy vs `LOCAL_BOT_THRESHOLD`, persist
//! the choice in `meetings.data` (survives restarts — the in-memory map didn't), and resolve it
//! back out for every later container operation on that meeting.

use crate::config::Config;
use crate::models::Meeting;
use sqlx::PgPool;

const ACTIVE_STATUSES: &[&str] = &["requested", "joining", "awaiting_admission", "active"];

/// Choose which runtime-api backend a *new* bot should spawn on.
pub async fn choose_backend_for_spawn(db: &PgPool, config: &Config) -> anyhow::Result<&'static str> {
    if !config.use_local_resource {
        return Ok("runpod");
    }
    let local_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meetings \
         WHERE platform != 'browser_session' \
           AND status = ANY($1) \
           AND data->>'runtime_backend' = 'local'",
    )
    .bind(ACTIVE_STATUSES)
    .fetch_one(db)
    .await?;

    Ok(if local_count < config.local_bot_threshold { "local" } else { "runpod" })
}

/// Resolve the already-decided backend for an existing meeting. Falls back to the static
/// `USE_LOCAL_RESOURCE` flag only for meetings created before this field existed.
pub fn backend_url_for<'a>(config: &'a Config, meeting: &Meeting) -> &'a str {
    let backend = meeting.runtime_backend().unwrap_or(if config.use_local_resource { "local" } else { "runpod" });
    backend_url_for_name(config, backend)
}

pub fn backend_url_for_name<'a>(config: &'a Config, backend: &str) -> &'a str {
    if backend == "local" {
        &config.local_backend_url
    } else {
        &config.runpod_backend_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(use_local: bool, threshold: i64) -> Config {
        Config {
            db_host: String::new(),
            db_port: 5432,
            db_name: String::new(),
            db_user: String::new(),
            db_password: String::new(),
            db_pool_size: 1,
            redis_url: String::new(),
            meeting_api_url: String::new(),
            bot_meeting_api_url: String::new(),
            bot_redis_url: String::new(),
            bot_image_name: String::new(),
            local_backend_url: "http://local".to_string(),
            runpod_backend_url: "http://runpod".to_string(),
            use_local_resource: use_local,
            local_bot_threshold: threshold,
            transcription_collector_url: String::new(),
            hivemind_url: String::new(),
            api_keys: vec![],
            internal_api_secret: String::new(),
            dev_mode: false,
            bot_stop_delay_seconds: 90,
            cors_origins: vec![],
        }
    }

    #[test]
    fn backend_url_for_name_picks_local_or_runpod() {
        let config = test_config(true, 3);
        assert_eq!(backend_url_for_name(&config, "local"), "http://local");
        assert_eq!(backend_url_for_name(&config, "runpod"), "http://runpod");
        // anything unrecognized falls to runpod, matching Python's `backend_url()`
        // ("local" if backend == "local" else RUNPOD) exactly
        assert_eq!(backend_url_for_name(&config, "bogus"), "http://runpod");
    }

    #[test]
    fn backend_url_for_falls_back_to_static_flag_when_meeting_has_no_backend_recorded() {
        let config_local_default = test_config(true, 3);
        let config_runpod_default = test_config(false, 3);
        let meeting = Meeting {
            id: 1,
            user_id: 1,
            platform: "google_meet".to_string(),
            platform_specific_id: Some("abc".to_string()),
            status: "active".to_string(),
            bot_container_id: None,
            start_time: None,
            end_time: None,
            data: serde_json::json!({}),
            created_at: None,
            updated_at: None,
        };
        assert_eq!(backend_url_for(&config_local_default, &meeting), "http://local");
        assert_eq!(backend_url_for(&config_runpod_default, &meeting), "http://runpod");
    }
}
