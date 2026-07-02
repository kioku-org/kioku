// config.rs
pub const REPO: &str = "kioku-org/kioku";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const DEFAULT_SERVER_URL: &str = "https://api.kioku.chat";
pub const DEFAULT_DASHBOARD_URL: &str = "https://dashboard.kioku.chat";

pub fn resolve_server_url(server_override: Option<&str>) -> String {
    resolve_server_url_from(
        server_override,
        std::env::var("KIOKU_SERVER").ok().as_deref(),
    )
}

pub fn resolve_server_url_from(
    server_override: Option<&str>,
    env_server: Option<&str>,
) -> String {
    server_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| env_server.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(DEFAULT_SERVER_URL)
        .to_string()
}

pub fn resolve_dashboard_url(server_url: &str) -> String {
    if let Ok(v) = std::env::var("KIOKU_DASHBOARD") {
        return v;
    }

    if server_url.contains("api.kioku.chat") {
        return DEFAULT_DASHBOARD_URL.to_string();
    }

    if let Some(stripped) = server_url
        .strip_prefix("http://localhost:")
        .or_else(|| server_url.strip_prefix("http://127.0.0.1:"))
    {
        let prefix = &server_url[..server_url.len() - stripped.len()];
        return format!("{}3001", prefix);
    }

    DEFAULT_DASHBOARD_URL.to_string()
}