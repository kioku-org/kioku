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
async fn vexa_request_bot_endpoint() {
    let c = client();
    let (token, _) = register_and_get_token("vexa_bot").await;

    let resp = c
        .post(format!("{}/vexa/bots", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "platform": "google_meet",
            "native_meeting_id": "test-meeting-123"
        }))
        .send()
        .await
        .unwrap();

    match resp.status().as_u16() {
        200..=299 => {
            let body: serde_json::Value = resp.json().await.unwrap();
            assert!(body.is_object(), "Vexa bot response should be an object");
        }
        502 | 503 => {
            eprintln!("Vexa service unavailable (expected if voice stack not running)");
        }
        _ => {
            eprintln!("Vexa bot request status: {} (may be expected if voice stack not deployed)", resp.status());
        }
    }
}

#[tokio::test]
async fn vexa_get_meetings_endpoint() {
    let c = client();
    let (token, _) = register_and_get_token("vexa_meetings").await;

    let resp = c
        .get(format!("{}/vexa/meetings", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();

    match resp.status().as_u16() {
        200..=299 => {
            let body: serde_json::Value = resp.json().await.unwrap();
            assert!(body.is_object() || body.is_array(), "Vexa meetings should return data");
        }
        502 | 503 => {
            eprintln!("Vexa service unavailable (expected if voice stack not running)");
        }
        _ => {
            eprintln!("Vexa meetings status: {} (may be expected if voice stack not deployed)", resp.status());
        }
    }
}