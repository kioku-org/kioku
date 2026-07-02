use cc_auth::AuthFile;
use serde_json::json;
use std::fs;

fn fixture_auth() -> AuthFile {
    AuthFile {
        server_url: "https://api.example.com".to_string(),
        token: "test-token-abc123".to_string(),
        user_id: "user-1".to_string(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        workspace_id: "comp-1".to_string(),
        role: "admin".to_string(),
        active_workspace_id: None,
    }
}

#[test]
fn auth_file_roundtrip() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("auth.json");

    let auth = fixture_auth();
    let json = serde_json::to_string_pretty(&auth).expect("serialize");
    fs::write(&path, &json).expect("write file");

    let loaded: AuthFile =
        serde_json::from_str(&fs::read_to_string(&path).expect("read file")).expect("deserialize");

    assert_eq!(loaded.server_url, auth.server_url);
    assert_eq!(loaded.token, auth.token);
    assert_eq!(loaded.user_id, auth.user_id);
    assert_eq!(loaded.email, auth.email);
    assert_eq!(loaded.name, auth.name);
    assert_eq!(loaded.workspace_id, auth.workspace_id);
    assert_eq!(loaded.role, auth.role);
}

#[test]
fn auth_file_serializes_all_fields() {
    let auth = fixture_auth();
    let json = serde_json::to_value(&auth).expect("serialize");

    let expected = json!({
        "server_url": "https://api.example.com",
        "token": "test-token-abc123",
        "user_id": "user-1",
        "email": "test@example.com",
        "name": "Test User",
        "workspace_id": "comp-1",
        "role": "admin",
        "active_workspace_id": null
    });

    assert_eq!(json, expected);
}

#[test]
fn auth_file_deserializes_from_json() {
    // Deliberately omits `active_workspace_id` — this is what auth.json
    // files written before multi-workspace support look like, and they
    // must keep loading fine.
    let json = json!({
        "server_url": "http://localhost:9100",
        "token": "tok-123",
        "user_id": "u-42",
        "email": "dev@kioku.chat",
        "name": "Dev",
        "workspace_id": "c-99",
        "role": "member"
    });

    let auth: AuthFile = serde_json::from_value(json).expect("deserialize");

    assert_eq!(auth.server_url, "http://localhost:9100");
    assert_eq!(auth.token, "tok-123");
    assert_eq!(auth.user_id, "u-42");
    assert_eq!(auth.email, "dev@kioku.chat");
    assert_eq!(auth.name, "Dev");
    assert_eq!(auth.workspace_id, "c-99");
    assert_eq!(auth.role, "member");
    assert_eq!(auth.active_workspace_id, None);
}
