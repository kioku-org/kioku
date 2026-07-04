use crate::auth::{resolve_token, scope_violation, user_scopes};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

const EXCLUDED_REQUEST_HEADERS: &[&str] = &["host", "content-length", "transfer-encoding"];
const STRIPPED_IDENTITY_HEADERS: &[&str] = &[
    "x-user-id",
    "x-user-scopes",
    "x-user-limits",
    "x-user-webhook-url",
    "x-user-webhook-secret",
    "x-user-webhook-events",
];
const EXCLUDED_RESPONSE_HEADERS: &[&str] = &["transfer-encoding", "connection", "content-length"];

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "detail": detail }))).into_response()
}

/// Faithful port of Python's `forward_request`: strips hop-by-hop + spoofable identity
/// headers, resolves the client's API key to inject `x-user-*` headers (or validates the
/// admin key), enforces route scopes, then proxies the request body verbatim to `target_url`.
pub async fn forward(state: &AppState, req: Request<Body>, target_url: String, require_auth: bool) -> Response {
    let method = req.method().clone();
    let req_path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());
    let orig_headers = req.headers().clone();

    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Failed to read body: {e}")),
    };

    let mut headers = HeaderMap::new();
    for (name, value) in orig_headers.iter() {
        let lname = name.as_str().to_lowercase();
        if EXCLUDED_REQUEST_HEADERS.contains(&lname.as_str()) || STRIPPED_IDENTITY_HEADERS.contains(&lname.as_str()) {
            continue;
        }
        headers.insert(name.clone(), value.clone());
    }

    let is_admin_request = target_url.starts_with(&format!("{}/admin", state.config.admin_api_url));

    if is_admin_request {
        if let Some(admin_key) = orig_headers.get("x-admin-api-key") {
            headers.insert(HeaderName::from_static("x-admin-api-key"), admin_key.clone());
        }
    } else {
        let client_key = orig_headers.get("x-api-key").and_then(|v| v.to_str().ok()).map(str::to_string);

        if require_auth && client_key.is_none() {
            return json_error(StatusCode::UNAUTHORIZED, "Missing API key");
        }

        if let Some(client_key) = client_key {
            if let Ok(v) = HeaderValue::from_str(&client_key) {
                headers.insert(HeaderName::from_static("x-api-key"), v);
            }

            match resolve_token(state, &client_key).await {
                Some(user_data) => {
                    let scopes = user_scopes(&user_data);
                    if scope_violation(&req_path, &scopes) {
                        return json_error(StatusCode::FORBIDDEN, "Insufficient scope for this endpoint");
                    }

                    if let Some(uid) = user_data.get("user_id") {
                        insert_str(&mut headers, "x-user-id", uid.to_string().trim_matches('"'));
                    }
                    insert_str(&mut headers, "x-user-scopes", &scopes.join(","));
                    let max_concurrent = user_data.get("max_concurrent").and_then(|v| v.as_i64()).unwrap_or(1);
                    insert_str(&mut headers, "x-user-limits", &max_concurrent.to_string());

                    if let Some(wh_url) = user_data.get("webhook_url").and_then(|v| v.as_str()) {
                        insert_str(&mut headers, "x-user-webhook-url", wh_url);
                        if let Some(secret) = user_data.get("webhook_secret").and_then(|v| v.as_str()) {
                            insert_str(&mut headers, "x-user-webhook-secret", secret);
                        }
                        if let Some(events) = user_data.get("webhook_events").and_then(|v| v.as_object()) {
                            let enabled: Vec<&str> = events
                                .iter()
                                .filter(|(_, on)| on.as_bool().unwrap_or(false))
                                .map(|(k, _)| k.as_str())
                                .collect();
                            if !enabled.is_empty() {
                                insert_str(&mut headers, "x-user-webhook-events", &enabled.join(","));
                            }
                        }
                    }
                }
                None => {
                    if require_auth {
                        return json_error(StatusCode::UNAUTHORIZED, "Invalid API key");
                    }
                }
            }
        }
    }

    let mut url = target_url;
    if let Some(q) = query {
        url.push('?');
        url.push_str(&q);
    }

    let resp = match state
        .http
        .request(method, &url)
        .headers(headers)
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return json_error(StatusCode::SERVICE_UNAVAILABLE, &format!("Service unavailable: {e}")),
    };

    build_response(resp).await
}

fn insert_str(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(HeaderName::from_static(name), v);
    }
}

/// Turn a reqwest response into an axum response, dropping hop-by-hop headers.
pub async fn build_response(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let mut out_headers = HeaderMap::new();
    for (name, value) in resp.headers().iter() {
        if !EXCLUDED_RESPONSE_HEADERS.contains(&name.as_str()) {
            out_headers.insert(name.clone(), value.clone());
        }
    }
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::BAD_GATEWAY, &format!("Failed reading upstream response: {e}")),
    };
    (status, out_headers, body).into_response()
}
