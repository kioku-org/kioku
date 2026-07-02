use crate::session::require_auth;
use anyhow::Result;

pub async fn run() -> Result<()> {
    let auth = require_auth()?;
    println!("{}", mcp_config_json(&auth.server_url, &auth.token));
    Ok(())
}

pub fn meeting_mcp_url(server_url: &str) -> String {
    let trimmed = server_url.trim_end_matches('/');

    if let Some(pos) = trimmed.rfind(':') {
        let after_colon = &trimmed[pos + 1..];

        if after_colon.chars().all(|c| c.is_ascii_digit()) {
            let prefix = &trimmed[..pos + 1];
            return format!("{}18888/mcp", prefix);
        }
    }

    format!("{}/meeting-mcp/mcp", trimmed)
}

pub fn mcp_config_json(server_url: &str, token: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "Kioku": {
                "url": format!("{}/mcp", server_url.trim_end_matches('/')),
                "headers": {
                    "Authorization": format!("Bearer {}", token)
                }
            },
            "Kioku Meetings": {
                "url": meeting_mcp_url(server_url),
                "headers": {
                    "Authorization": format!("Bearer {}", token)
                }
            }
        }
    }))
    .expect("json serialization is infallible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_json_contains_both_servers() {
        let json_str = mcp_config_json("http://localhost:9100", "test-token");
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let servers = &v["mcpServers"];
        assert_eq!(servers["Kioku"]["url"], "http://localhost:9100/mcp");
        assert_eq!(
            servers["Kioku"]["headers"]["Authorization"],
            "Bearer test-token"
        );
        assert_eq!(
            servers["Kioku Meetings"]["url"],
            "http://localhost:18888/mcp"
        );
        assert_eq!(
            servers["Kioku Meetings"]["headers"]["Authorization"],
            "Bearer test-token"
        );
    }

    #[test]
    fn meeting_mcp_url_replaces_port() {
        assert_eq!(
            meeting_mcp_url("http://localhost:9100"),
            "http://localhost:18888/mcp"
        );
        assert_eq!(
            meeting_mcp_url("http://localhost:9100/"),
            "http://localhost:18888/mcp"
        );
    }

    #[test]
    fn meeting_mcp_url_no_port_returns_placeholder() {
        let url = meeting_mcp_url("https://api.kioku.chat");
        assert!(url.contains("mcp"), "should contain mcp in URL: {url}");
    }
}
