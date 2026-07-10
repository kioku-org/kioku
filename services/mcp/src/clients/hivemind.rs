use serde_json::{json, Value};

#[derive(Clone)]
pub struct HivemindClient {
    http: reqwest::Client,
    base_url: String,
}

impl HivemindClient {
    pub(crate) fn new(http: reqwest::Client, base_url: String) -> Self {
        Self { http, base_url }
    }

    pub async fn http_request(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url).bearer_auth(token);

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        let status = resp.status();

        let body: Value = resp.json().await.unwrap_or(json!({}));

        if !status.is_success() {
            return Err(format!("HTTP {status}: {body}"));
        }

        Ok(body)
    }
}
