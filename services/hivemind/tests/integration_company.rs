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
    assert!(
        resp.status().is_success(),
        "Register failed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    (body["token"].as_str().unwrap().to_string(), body)
}

#[tokio::test]
async fn company_config_get() {
    let c = client();
    let (token, _) = register_and_get_token("cfg_get").await;
    let resp = c
        .get(format!("{}/company/config", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Get config failed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("company_id").is_some());
    assert!(body.get("default_provider").is_some());
}

#[tokio::test]
async fn company_config_update() {
    let c = client();
    let (token, _) = register_and_get_token("cfg_upd").await;
    let resp = c
        .put(format!("{}/company/config", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "default_provider": "openai",
            "default_model": "gpt-4o"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Update config failed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["default_provider"], "openai");
    assert_eq!(body["default_model"], "gpt-4o");
}

#[tokio::test]
async fn invite_lifecycle() {
    let c = client();
    let (token, _) = register_and_get_token("invite").await;
    let invite_email = format!("member_{}@example.com", uuid::Uuid::new_v4());

    let create = c
        .post(format!("{}/company/invites", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "email": &invite_email,
            "role": "member"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        create.status().is_success(),
        "Create invite failed: {}",
        create.status()
    );
    let invite: serde_json::Value = create.json().await.unwrap();
    let invite_id = invite["id"].as_str().unwrap();

    let list = c
        .get(format!("{}/company/invites", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert!(list_body.as_array().unwrap().len() >= 1);

    let del = c
        .delete(format!("{}/company/invites/{}", base_url(), invite_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(
        del.status().is_success(),
        "Delete invite failed: {}",
        del.status()
    );
}

#[tokio::test]
async fn member_list() {
    let c = client();
    let (token, _) = register_and_get_token("members").await;
    let resp = c
        .get(format!("{}/company/members", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.as_array().unwrap().len() >= 1,
        "Admin should be a member"
    );
}

#[tokio::test]
async fn api_key_lifecycle() {
    let c = client();
    let (token, reg_body) = register_and_get_token("apikey").await;
    let user_id = reg_body["user_id"].as_str().unwrap();

    let create = c
        .post(format!("{}/company/apikeys", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "user_id": user_id,
            "provider": "anthropic",
            "plain_key": "sk-test-12345"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        create.status().is_success(),
        "Create API key failed: {}",
        create.status()
    );
    let key_resp: serde_json::Value = create.json().await.unwrap();
    assert!(key_resp["key_masked"].as_str().unwrap().contains("..."));

    let list = c
        .get(format!("{}/company/apikeys/{}", base_url(), user_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(list.status().is_success());
    let keys: serde_json::Value = list.json().await.unwrap();
    assert!(keys.as_array().unwrap().len() >= 1);

    let key_id = key_resp["id"].as_str().unwrap();
    let del = c
        .delete(format!("{}/company/apikeys/key/{}", base_url(), key_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert!(
        del.status().is_success(),
        "Delete API key failed: {}",
        del.status()
    );
}
