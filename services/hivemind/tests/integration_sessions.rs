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
            "workspace_name": format!("{} Company", suffix),
            "email": &email,
            "name": format!("{} User", suffix),
            "password": "testpassword123"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Register failed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    (body["token"].as_str().unwrap().to_string(), body)
}

#[tokio::test]
async fn session_crud() {
    let c = client();
    let (token, _) = register_and_get_token("sess_crud").await;

    let create = c
        .post(format!("{}/sessions", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "title": "Test Session",
            "cwd": "/home/user",
            "mode": "research"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        create.status().is_success(),
        "Create session failed: {}",
        create.status()
    );
    let session: serde_json::Value = create.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();
    assert_eq!(session["title"], "Test Session");
    assert_eq!(session["mode"], "research");

    let get = c
        .get(format!("{}/sessions/{}", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(get.status().is_success());
    let gotten: serde_json::Value = get.json().await.unwrap();
    assert_eq!(gotten["id"], session_id);

    let list = c
        .get(format!("{}/sessions", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
    let sessions: serde_json::Value = list.json().await.unwrap();
    assert!(sessions.as_array().unwrap().len() >= 1);

    let update = c
        .patch(format!("{}/sessions/{}", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "title": "Updated Session",
            "status": "running"
        }))
        .send()
        .await
        .unwrap();
    assert!(update.status().is_success());
    let updated: serde_json::Value = update.json().await.unwrap();
    assert_eq!(updated["title"], "Updated Session");
    assert_eq!(updated["status"], "running");

    let delete = c
        .delete(format!("{}/sessions/{}", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(delete.status().is_success());

    let get_after = c
        .get(format!("{}/sessions/{}", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(get_after.status(), 404, "Deleted session should be 404");
}

#[tokio::test]
async fn message_lifecycle() {
    let c = client();
    let (token, _) = register_and_get_token("msg_life").await;

    let create_session = c
        .post(format!("{}/sessions", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({"title": "Message Test"}))
        .send()
        .await
        .unwrap();
    let session: serde_json::Value = create_session.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    let msg = c
        .post(format!("{}/sessions/{}/messages", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "user",
            "content": [{"type": "text", "text": "Hello"}],
            "timestamp": 1700000000000_i64
        }))
        .send()
        .await
        .unwrap();
    assert!(
        msg.status().is_success(),
        "Create message failed: {}",
        msg.status()
    );

    let list = c
        .get(format!("{}/sessions/{}/messages", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
    let messages: serde_json::Value = list.json().await.unwrap();
    assert!(messages.as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn trace_lifecycle() {
    let c = client();
    let (token, _) = register_and_get_token("trace_life").await;

    let create_session = c
        .post(format!("{}/sessions", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({"title": "Trace Test"}))
        .send()
        .await
        .unwrap();
    let session: serde_json::Value = create_session.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    let trace = c
        .post(format!("{}/sessions/{}/traces", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "type": "tool_use",
            "status": "running",
            "title": "Reading file",
            "tool_name": "read",
            "timestamp": 1700000000000_i64
        }))
        .send()
        .await
        .unwrap();
    assert!(
        trace.status().is_success(),
        "Create trace failed: {}",
        trace.status()
    );
    let trace_body: serde_json::Value = trace.json().await.unwrap();
    let trace_id = trace_body["id"].as_str().unwrap();

    let update = c
        .patch(format!(
            "{}/sessions/{}/traces/{}",
            base_url(),
            session_id,
            trace_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "status": "done",
            "tool_output": "file contents here"
        }))
        .send()
        .await
        .unwrap();
    assert!(update.status().is_success());

    let list = c
        .get(format!("{}/sessions/{}/traces", base_url(), session_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
}
