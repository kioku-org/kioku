#[cfg(test)]
mod auth_tests {
    use reqwest::Client;
    use serde_json::json;

     fn client() -> Client {
        reqwest::Client::new()
    }

    fn base_url() -> String {
        std::env::var("HIVEMIND_URL").unwrap_or_else(|_| "http://localhost:9100".into())
    }

    #[tokio::test]
    async fn health_check() {
        let resp = client().get(format!("{}/health", base_url())).send().await;
        match resp {
            Ok(r) => {
                assert!(r.status().is_success(), "Health check failed: {}", r.status());
                let body: serde_json::Value = r.json().await.unwrap();
                assert_eq!(body["status"], "ok");
            }
            Err(e) => panic!("Hivemind not reachable at {}: {:?}", base_url(), e),
        }
    }

    #[tokio::test]
    async fn register_admin_success() {
        let c = client();
        let email = format!("test_admin_{}@example.com", uuid::Uuid::new_v4());
        let resp = c
            .post(format!("{}/auth/register/admin", base_url()))
            .json(&json!({
                "company_name": "Test Company",
                "email": email,
                "name": "Test Admin",
                "password": "testpassword123"
            }))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success(), "Register admin failed: {}", resp.status());
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body.get("token").is_some(), "Missing token in response");
        assert_eq!(body["role"], "admin");
        assert_eq!(body["email"], email);
    }

    #[tokio::test]
    async fn register_admin_short_password() {
        let c = client();
        let resp = c
            .post(format!("{}/auth/register/admin", base_url()))
            .json(&json!({
                "company_name": "Test Co",
                "email": "short@example.com",
                "name": "Test",
                "password": "abc"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "Short password should be rejected");
    }

    #[tokio::test]
    async fn register_admin_duplicate_email() {
        let c = client();
        let email = format!("dup_{}@example.com", uuid::Uuid::new_v4());
        let body = json!({
            "company_name": "Dup Co",
            "email": &email,
            "name": "Dup User",
            "password": "testpassword123"
        });
        let resp1 = c.post(format!("{}/auth/register/admin", base_url())).json(&body).send().await.unwrap();
        assert!(resp1.status().is_success());
        let resp2 = c.post(format!("{}/auth/register/admin", base_url())).json(&body).send().await.unwrap();
        assert!(resp2.status().is_client_error(), "Duplicate email should fail");
    }

    #[tokio::test]
    async fn signin_success() {
        let c = client();
        let email = format!("signin_{}@example.com", uuid::Uuid::new_v4());
        let reg = c
            .post(format!("{}/auth/register/admin", base_url()))
            .json(&json!({
                "company_name": "Signin Co",
                "email": &email,
                "name": "Signin User",
                "password": "testpassword123"
            }))
            .send()
            .await
            .unwrap();
        assert!(reg.status().is_success());

        let signin = c
            .post(format!("{}/auth/signin", base_url()))
            .json(&json!({
                "email": &email,
                "password": "testpassword123"
            }))
            .send()
            .await
            .unwrap();
        assert!(signin.status().is_success(), "Signin failed: {}", signin.status());
        let body: serde_json::Value = signin.json().await.unwrap();
        assert!(body.get("token").is_some());
    }

    #[tokio::test]
    async fn signin_wrong_password() {
        let c = client();
        let resp = c
            .post(format!("{}/auth/signin", base_url()))
            .json(&json!({
                "email": "nonexistent@example.com",
                "password": "wrongpassword"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "Wrong password should return 401");
    }

    #[tokio::test]
    async fn auth_me_with_valid_token() {
        let c = client();
        let email = format!("me_{}@example.com", uuid::Uuid::new_v4());
        let reg = c
            .post(format!("{}/auth/register/admin", base_url()))
            .json(&json!({
                "company_name": "Me Co",
                "email": &email,
                "name": "Me User",
                "password": "testpassword123"
            }))
            .send()
            .await
            .unwrap();
        let reg_body: serde_json::Value = reg.json().await.unwrap();
        let token = reg_body["token"].as_str().unwrap();

        let me = c
            .get(format!("{}/auth/me", base_url()))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .unwrap();
        assert!(me.status().is_success(), "/auth/me failed: {}", me.status());
        let me_body: serde_json::Value = me.json().await.unwrap();
        assert_eq!(me_body["email"], email);
    }

    #[tokio::test]
    async fn auth_me_without_token() {
        let c = client();
        let resp = c.get(format!("{}/auth/me", base_url())).send().await.unwrap();
        assert_eq!(resp.status(), 401, "Should require auth token");
    }

    #[tokio::test]
    async fn signout() {
        let c = client();
        let email = format!("out_{}@example.com", uuid::Uuid::new_v4());
        let reg = c
            .post(format!("{}/auth/register/admin", base_url()))
            .json(&json!({
                "company_name": "Out Co",
                "email": &email,
                "name": "Out User",
                "password": "testpassword123"
            }))
            .send()
            .await
            .unwrap();
        let reg_body: serde_json::Value = reg.json().await.unwrap();
        let token = reg_body["token"].as_str().unwrap();

        let out = c
            .post(format!("{}/auth/signout", base_url()))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .unwrap();
        assert!(out.status().is_success(), "Signout failed: {}", out.status());

        let me = c
            .get(format!("{}/auth/me", base_url()))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .unwrap();
        assert_eq!(me.status(), 401, "Token should be invalidated after signout");
    }
}