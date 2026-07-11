use rmcp::service::RequestContext;
use rmcp::RoleServer;
use serde_json::Value;

pub fn bearer_token_from_context(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<axum::http::request::Parts>()?;
    if let Some(auth) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        return Some(auth.strip_prefix("Bearer ").unwrap_or(auth).to_string());
    }
    parts
        .headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

pub async fn resolve_vexa_key(
    http: &reqwest::Client,
    hivemind_api_url: &str,
    token: &str,
) -> String {
    let url = format!("{hivemind_api_url}/vexa/token");
    match http
        .get(&url)
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(v) = resp.json::<Value>().await {
                if let Some(key) = v.get("vexa_api_key").and_then(|k| k.as_str()) {
                    return key.to_string();
                }
            }
            token.to_string()
        }
        _ => token.to_string(),
    }
}
