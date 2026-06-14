use reqwest::Client;
use serde_json::json;

fn base_url() -> String {
    std::env::var("HIVEMIND_URL").unwrap_or_else(|_| "http://localhost:9100".into())
}

 fn client() -> Client {
    reqwest::Client::new()
}

async fn register_and_get_token(suffix: &str) -> (String, serde_json::Value) {
    let c = client();
    let email = format!("{}_{}@example.com", suffix, uuid::Uuid::new_v4());
    let resp = c
        .post(format!("{}/auth/register/admin", base_url()))
        .json(&json!({
            "company_name": format!("{} Company", suffix),
            "email": &email,
            "name": format!("{} User", suffix),
            "password": "testpassword123"
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Register failed: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    (body["token"].as_str().unwrap().to_string(), body)
}

#[tokio::test]
async fn meeting_ingest_and_list() {
    let c = client();
    let (token, _) = register_and_get_token("meeting").await;
    let now = chrono::Utc::now().timestamp_millis();

    let ingest = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "title": "Weekly Standup",
            "date": now,
            "duration_seconds": 1800,
            "participants": ["Alice", "Bob"],
            "transcript": [
                {"speaker": "Alice", "text": "Hey everyone", "start_time": 0.0, "end_time": 2.5},
                {"speaker": "Bob", "text": "Hi Alice", "start_time": 2.5, "end_time": 5.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(ingest.status().is_success(), "Ingest meeting failed: {}", ingest.status());
    let meeting: serde_json::Value = ingest.json().await.unwrap();
    assert_eq!(meeting["title"], "Weekly Standup");
    assert!(meeting["id"].is_string());

    let list = c
        .get(format!("{}/meetings", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
    let meetings: serde_json::Value = list.json().await.unwrap();
    assert!(meetings.as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn usage_record_and_summary() {
    let c = client();
    let (token, reg_body) = register_and_get_token("usage").await;
    let user_id = reg_body["user_id"].as_str().unwrap();

    let record = c
        .post(format!("{}/usage", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "user_id": user_id,
            "session_id": "test-session-001",
            "model": "claude-sonnet-4-5",
            "provider": "anthropic",
            "input_tokens": 1000,
            "output_tokens": 500
        }))
        .send()
        .await
        .unwrap();
    assert!(record.status().is_success(), "Record usage failed: {}", record.status());

    let summary = c
        .get(format!("{}/usage/summary", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(summary.status().is_success(), "Usage summary failed: {}", summary.status());
    let summary_body: serde_json::Value = summary.json().await.unwrap();
    assert!(summary_body.as_array().unwrap().len() >= 1);
}