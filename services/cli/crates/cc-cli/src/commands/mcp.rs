use anyhow::Result;
use crate::session::require_auth;

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