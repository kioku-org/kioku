use std::env;

pub struct Config {
    pub admin_api_url: String,
    pub meeting_api_url: String,
    pub transcription_collector_url: String,
    pub mcp_url: String,
    pub calendar_service_url: Option<String>,
    pub agent_api_url: Option<String>,
    pub runtime_api_url: String,

    pub public_base_url: Option<String>,
    pub transcript_share_ttl_seconds: i64,
    pub transcript_share_ttl_max_seconds: i64,

    pub rate_limit_rpm: i64,
    pub rate_limit_admin_rpm: i64,
    pub rate_limit_ws_rpm: i64,

    pub redis_url: String,
    pub internal_api_secret: String,
    pub cors_origins: Vec<String>,
    pub vnc_host_override: Option<String>,
    pub is_production: bool,
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let missing: Vec<&str> = [
            ("ADMIN_API_URL", env::var("ADMIN_API_URL").ok()),
            ("MEETING_API_URL", env::var("MEETING_API_URL").ok()),
            (
                "TRANSCRIPTION_COLLECTOR_URL",
                env::var("TRANSCRIPTION_COLLECTOR_URL").ok(),
            ),
            ("MCP_URL", env::var("MCP_URL").ok()),
        ]
        .into_iter()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k)
        .collect();
        if !missing.is_empty() {
            anyhow::bail!("Missing required environment variables: {}", missing.join(", "));
        }

        let cors_raw = env::var("CORS_ORIGINS").unwrap_or_else(|_| "*".to_string());
        let cors_origins = if cors_raw.trim() == "*" {
            vec!["*".to_string()]
        } else {
            cors_raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };

        Ok(Self {
            admin_api_url: env::var("ADMIN_API_URL").unwrap(),
            meeting_api_url: env::var("MEETING_API_URL").unwrap(),
            transcription_collector_url: env::var("TRANSCRIPTION_COLLECTOR_URL").unwrap(),
            mcp_url: env::var("MCP_URL").unwrap(),
            calendar_service_url: env::var("CALENDAR_SERVICE_URL").ok(),
            agent_api_url: env::var("AGENT_API_URL").ok(),
            runtime_api_url: env::var("RUNTIME_API_URL")
                .unwrap_or_else(|_| "http://runtime-api:8090".to_string()),

            public_base_url: env::var("PUBLIC_BASE_URL").ok(),
            transcript_share_ttl_seconds: env_i64("TRANSCRIPT_SHARE_TTL_SECONDS", 900),
            transcript_share_ttl_max_seconds: env_i64("TRANSCRIPT_SHARE_TTL_MAX_SECONDS", 86400),

            rate_limit_rpm: env_i64("RATE_LIMIT_RPM", 120),
            rate_limit_admin_rpm: env_i64("RATE_LIMIT_ADMIN_RPM", 30),
            rate_limit_ws_rpm: env_i64("RATE_LIMIT_WS_RPM", 20),

            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://redis:6379/0".to_string()),
            internal_api_secret: env::var("INTERNAL_API_SECRET").unwrap_or_default(),
            cors_origins,
            vnc_host_override: env::var("VNC_HOST").ok(),
            is_production: env::var("VEXA_ENV").unwrap_or_else(|_| "development".to_string()) == "production",
        })
    }
}

/// Route path prefix -> required scopes (token needs at least one to pass).
/// Mirrors Python's ROUTE_SCOPES.
pub const ROUTE_SCOPES: &[(&str, &[&str])] = &[
    ("/user/", &["bot"]),
    ("/bots", &["bot", "browser"]),
    ("/b/", &["browser"]),
    ("/transcripts", &["tx"]),
    ("/meetings", &["tx"]),
];
