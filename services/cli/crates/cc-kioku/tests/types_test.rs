use cc_kioku::*;
use serde_json::json;

#[test]
fn auth_session_roundtrip() {
    let json = json!({
        "user_id": "u-1",
        "email": "admin@kioku.chat",
        "name": "Admin",
        "company_id": "c-1",
        "company_name": "Kioku",
        "company_slug": "kioku",
        "role": "admin",
        "token": "jwt-token-here"
    });

    let session: AuthSession = serde_json::from_value(json.clone()).expect("deserialize");
    let re_serialized = serde_json::to_value(&session).expect("serialize");

    assert_eq!(re_serialized, json);
}

#[test]
fn session_defaults() {
    let json = json!({
        "id": "s-1",
        "company_id": "c-1",
        "user_id": "u-1",
        "title": "Test Session",
        "status": "active",
        "mode": "research",
        "created_at": 1700000000,
        "updated_at": 1700000000
    });

    let session: Session = serde_json::from_value(json).expect("deserialize");

    assert_eq!(session.cwd, None);
}

#[test]
fn knowledge_search_result_nested() {
    let json = json!({
        "chunk": {"text": "This is relevant context", "meeting_id": "m-1"},
        "meeting": {"title": "Standup", "date": 1700000000},
        "score": 0.95
    });

    let result: KnowledgeSearchResult = serde_json::from_value(json).expect("deserialize");

    assert!((result.score - 0.95).abs() < f64::EPSILON);
    assert_eq!(result.chunk["text"], "This is relevant context");
    assert_eq!(result.meeting["title"], "Standup");
}

#[test]
fn meeting_ingest_defaults() {
    let json = json!({
        "title": "Standup",
        "date": 1700000000
    });

    let req: MeetingIngestRequest = serde_json::from_value(json).expect("deserialize");

    assert_eq!(req.title, "Standup");
    assert_eq!(req.duration_seconds, 0);
    assert!(req.participants.is_empty());
    assert!(req.transcript.is_empty());
}

#[test]
fn company_auth_key_optional_fields() {
    let json = json!({
        "id": "k-1",
        "user_id": "u-1",
        "name": "ci-key",
        "key_prefix": "koku_abc",
        "created_at": 1700000000
    });

    let key: CompanyAuthKeyOut = serde_json::from_value(json).expect("deserialize");

    assert_eq!(key.key_prefix, "koku_abc");
    assert_eq!(key.last_used_at, None);
}

#[test]
fn api_error_format() {
    let json = json!({
        "error": "unauthorized",
        "ok": false
    });

    let err: ApiError = serde_json::from_value(json).expect("deserialize");

    assert_eq!(err.error, "unauthorized");
    assert!(!err.ok);
}

#[test]
fn knowledge_search_request_defaults() {
    let json = json!({"query":"test"});

    let req: KnowledgeSearchRequest = serde_json::from_value(json).expect("deserialize");

    assert_eq!(req.query, "test");
    assert_eq!(req.limit, 5);
}

#[test]
fn content_part_text() {
    let json = json!({
        "type": "text",
        "text": "Hello world"
    });

    let part: ContentPart = serde_json::from_value(json).expect("deserialize");

    assert_eq!(part.part_type, "text");
    assert_eq!(part.text, Some("Hello world".to_string()));
}

#[test]
fn message_roundtrip() {
    let json = json!({
        "id": "m-1",
        "session_id": "s-1",
        "role": "assistant",
        "content": [{"type":"text","text":"response"}],
        "timestamp": 1700000000
    });

    let msg: Message = serde_json::from_value(json).expect("deserialize");

    assert_eq!(msg.role, "assistant");
}

#[test]
fn upload_response_fields() {
    let json = json!({
        "id": "doc-1",
        "filename": "report.pdf",
        "status": "processing"
    });

    let resp: UploadResponse = serde_json::from_value(json).expect("deserialize");

    assert_eq!(resp.id, "doc-1");
    assert_eq!(resp.filename, "report.pdf");
    assert_eq!(resp.status, "processing");
}

#[test]
fn company_config_minimal() {
    let json = json!({
        "hivemind_enabled": true,
        "updated_at": 1700000000
    });

    let config: CompanyConfig = serde_json::from_value(json).expect("deserialize");

    assert!(config.hivemind_enabled);
    assert_eq!(config.updated_at, 1700000000);
}
