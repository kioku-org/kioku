use std::env;

pub struct Config {
    pub kioku_api_url: String,
    pub hivemind_api_url: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            kioku_api_url: env::var("KIOKU_API_URL").unwrap_or_else(|_| "http://api-gateway:8000".to_string()),
            hivemind_api_url: env::var("HIVEMIND_API_URL").unwrap_or_else(|_| "http://localhost:9100".to_string()),
            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(18888),
        }
    }
}
