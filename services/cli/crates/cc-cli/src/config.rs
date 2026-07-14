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

pub fn resolve_server_url_from(server_override: Option<&str>, env_server: Option<&str>) -> String {
    server_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| env_server.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(DEFAULT_SERVER_URL)
        .to_string()
}

/// Vexa gateway (meetings API) base URL for a given kioku server. Same shape
/// as [`resolve_dashboard_url`]: env override, hosted default, localhost port map.
pub fn resolve_meetings_url(server_url: &str) -> anyhow::Result<String> {
    if let Ok(v) = std::env::var("KIOKU_MEETINGS") {
        return Ok(v);
    }
    if server_url.contains("api.kioku.chat") {
        return Ok("https://meetings.kioku.chat".to_string());
    }
    if let Some(stripped) = server_url
        .strip_prefix("http://localhost:")
        .or_else(|| server_url.strip_prefix("http://127.0.0.1:"))
    {
        let prefix = &server_url[..server_url.len() - stripped.len()];
        return Ok(format!("{}8056", prefix));
    }
    anyhow::bail!(
        "cannot derive the meetings gateway URL from server `{server_url}` — set KIOKU_MEETINGS to your gateway base URL (e.g. https://meetings.example.com)"
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_server_url_prefers_cli_override() {
        let actual =
            resolve_server_url_from(Some("https://cli.example"), Some("https://env.example"));
        let expected = "https://cli.example".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_server_url_uses_env_when_cli_missing() {
        let actual = resolve_server_url_from(None, Some("https://env.example"));
        let expected = "https://env.example".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_server_url_defaults_to_local_hivemind() {
        let actual = resolve_server_url_from(None, None);
        let expected = DEFAULT_SERVER_URL.to_string();

        assert_eq!(actual, expected);
    }
}
