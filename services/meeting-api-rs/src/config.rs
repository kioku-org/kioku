use std::env;

pub struct Config {
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_password: String,
    pub db_pool_size: u32,

    pub redis_url: String,

    pub meeting_api_url: String,
    pub bot_meeting_api_url: String,
    pub bot_redis_url: String,
    pub bot_image_name: String,

    /// Two runtime-api instances this service load-balances bot spawns across, replacing the
    /// old standalone `router` service. See runtime_backend.rs.
    pub local_backend_url: String,
    pub runpod_backend_url: String,
    pub use_local_resource: bool,
    pub local_bot_threshold: i64,

    pub transcription_collector_url: String,
    pub hivemind_url: String,

    pub api_keys: Vec<String>,
    pub internal_api_secret: String,
    pub dev_mode: bool,

    pub bot_stop_delay_seconds: i64,
    pub cors_origins: Vec<String>,
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let missing: Vec<&str> = [
            ("DB_HOST", env::var("DB_HOST").ok()),
            ("DB_PORT", env::var("DB_PORT").ok()),
            ("DB_NAME", env::var("DB_NAME").ok()),
            ("DB_USER", env::var("DB_USER").ok()),
            ("DB_PASSWORD", env::var("DB_PASSWORD").ok()),
            ("REDIS_URL", env::var("REDIS_URL").ok()),
        ]
        .into_iter()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k)
        .collect();
        if !missing.is_empty() {
            anyhow::bail!("Missing required environment variables: {}", missing.join(", "));
        }

        let meeting_api_url = env_or("MEETING_API_URL", "http://meeting-api:8080");
        let cors_raw = env_or("CORS_ORIGINS", "*");
        let cors_origins = if cors_raw.trim() == "*" {
            vec!["*".to_string()]
        } else {
            cors_raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };

        Ok(Self {
            db_host: env::var("DB_HOST").unwrap(),
            db_port: env::var("DB_PORT").unwrap().parse().unwrap_or(5432),
            db_name: env::var("DB_NAME").unwrap(),
            db_user: env::var("DB_USER").unwrap(),
            db_password: env::var("DB_PASSWORD").unwrap(),
            db_pool_size: env_or("DB_POOL_SIZE", "20").parse().unwrap_or(20),

            redis_url: env::var("REDIS_URL").unwrap(),

            bot_meeting_api_url: env_or("BOT_MEETING_API_URL", &meeting_api_url),
            bot_redis_url: env::var("BOT_REDIS_URL").unwrap_or_else(|_| env::var("REDIS_URL").unwrap()),
            bot_image_name: env_or("BOT_IMAGE_NAME", "vexaai/vexa-bot:latest"),
            meeting_api_url,

            local_backend_url: env_or("LOCAL_BACKEND_URL", "http://localhost:8091"),
            runpod_backend_url: env_or("RUNPOD_BACKEND_URL", "http://localhost:8092"),
            use_local_resource: env_or("USE_LOCAL_RESOURCE", "true") == "true",
            local_bot_threshold: env_or("LOCAL_BOT_THRESHOLD", "3").parse().unwrap_or(3),

            transcription_collector_url: env_or(
                "TRANSCRIPTION_COLLECTOR_URL",
                "http://transcription-collector:8000",
            ),
            hivemind_url: env_or("HIVEMIND_URL", "http://localhost:9100"),

            api_keys: env_or("API_KEYS", "")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            internal_api_secret: env_or("INTERNAL_API_SECRET", ""),
            dev_mode: env_or("DEV_MODE", "false") == "true",

            bot_stop_delay_seconds: env_or("BOT_STOP_DELAY_SECONDS", "90").parse().unwrap_or(90),
            cors_origins,
        })
    }
}
