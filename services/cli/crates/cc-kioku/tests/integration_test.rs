#![cfg(feature = "integration")]

use cc_kioku::KiokuClient;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

static SETUP_DONE: AtomicBool = AtomicBool::new(false);

fn base_url() -> String {
    std::env::var("HIVEMIND_URL").unwrap_or_else(|_| "http://localhost:9100".into())
}

async fn setup() -> (KiokuClient, String) {
    let base = base_url();
    let client = KiokuClient::new(&base);

    if !SETUP_DONE.swap(true, Ordering::SeqCst) {
        // Register admin (ignore if already registered)
        let _ = client
            .register_admin(
                "cli-test-company",
                Some("cli-test"),
                "cli_test@example.com",
                "CLI Test User",
                "testpassword123",
            )
            .await;
    }

    let session = client
        .signin("cli_test@example.com", "testpassword123")
        .await
        .expect("signin failed");

    (
        KiokuClient::with_token(&base, &session.token),
        session.user_id,
    )
}

#[tokio::test]
async fn auth_lifecycle() {
    let (client, _) = setup().await;

    let me = client.whoami().await.expect("whoami failed");
    assert_eq!(me.email, "cli_test@example.com");
    assert_eq!(me.role, "admin");
}

#[tokio::test]
async fn session_crud() {
    let (client, _) = setup().await;

    let session = client
        .create_session("Integration Test Session", "research")
        .await
        .expect("create session failed");
    assert_eq!(session.title, "Integration Test Session");

    let fetched = client.get_session(&session.id).await.expect("get failed");
    assert_eq!(fetched.id, session.id);

    let sessions = client.list_sessions().await.expect("list failed");
    assert!(sessions.iter().any(|s| s.id == session.id));

    client
        .delete_session(&session.id)
        .await
        .expect("delete failed");

    let sessions = client.list_sessions().await.expect("list after delete");
    assert!(!sessions.iter().any(|s| s.id == session.id));
}

#[tokio::test]
async fn message_send_and_list() {
    let (client, _) = setup().await;

    let session = client
        .create_session("Message Test", "research")
        .await
        .unwrap();

    let msg = client
        .send_message(&session.id, "Hello from integration test")
        .await
        .expect("send failed");
    assert_eq!(msg.role, "user");

    let msgs = client
        .list_messages(&session.id)
        .await
        .expect("list failed");
    assert!(msgs.len() >= 1);

    client.delete_session(&session.id).await.unwrap();
}

#[tokio::test]
async fn meetings_ingest_and_list() {
    let (client, _) = setup().await;

    let ingest = cc_kioku::MeetingIngestRequest {
        title: "CLI Test Meeting".to_string(),
        date: chrono::Utc::now().timestamp_millis(),
        duration_seconds: 120,
        participants: vec!["Alice".to_string(), "Bob".to_string()],
        transcript: vec![
            cc_kioku::TranscriptSegment {
                speaker: "Alice".to_string(),
                text: "Hello from CLI test".to_string(),
                start_time: Some(0.0),
                end_time: Some(2.5),
            },
            cc_kioku::TranscriptSegment {
                speaker: "Bob".to_string(),
                text: "Hi Alice".to_string(),
                start_time: Some(2.5),
                end_time: Some(5.0),
            },
        ],
    };

    let meeting = client.ingest_meeting(&ingest).await.expect("ingest failed");
    assert_eq!(meeting.title, "CLI Test Meeting");

    let meetings = client.list_meetings().await.expect("list failed");
    assert!(meetings.iter().any(|m| m.id == meeting.id));
}

#[tokio::test]
async fn auth_keys_crud() {
    let (client, _) = setup().await;

    let key = client
        .create_auth_key("test-integration-key")
        .await
        .expect("create auth key failed");
    let key_id = key["id"].as_str().expect("missing id").to_string();

    let keys = client
        .list_auth_keys()
        .await
        .expect("list auth keys failed");
    assert!(keys.iter().any(|k| k.id == key_id));

    client
        .delete_auth_key(&key_id)
        .await
        .expect("delete auth key failed");

    let keys = client.list_auth_keys().await.expect("list after delete");
    assert!(!keys.iter().any(|k| k.id == key_id));
}

#[tokio::test]
async fn knowledge_search_empty() {
    let (client, _) = setup().await;

    let results = client
        .knowledge_search("nonexistent query xyz", 5)
        .await
        .expect("search failed");

    assert!(results.is_empty());
}
