use crate::types::*;
use anyhow::{Context, Result};
use reqwest::{multipart, Client, StatusCode};
use std::path::Path;

pub struct CompanionClient {
    base_url: String,
    client: Client,
    token: Option<String>,
}

impl CompanionClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
            token: None,
        }
    }

    pub fn with_token(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
            token: Some(token.to_string()),
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {}", t))
    }

    async fn handle_error(resp: reqwest::Response) -> anyhow::Error {
        let status = resp.status();
        match resp.text().await {
            Ok(body) => {
                if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                    anyhow::anyhow!("{}: {}", status, api_err.error)
                } else {
                    anyhow::anyhow!("{}: {}", status, body)
                }
            }
            Err(_) => anyhow::anyhow!("{}", status),
        }
    }

    pub async fn signin(&self, email: &str, password: &str) -> Result<AuthSession> {
        let resp = self
            .client
            .post(self.url("/auth/signin"))
            .json(&SignInRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .context("signin request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("invalid email or password"));
        }
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }

        resp.json::<AuthSession>()
            .await
            .context("invalid signin response")
    }

    pub async fn register_personal(&self, email: &str, name: &str, password: &str) -> Result<AuthSession> {
        let resp = self
            .client
            .post(self.url("/auth/register/personal"))
            .json(&RegisterPersonalRequest {
                email: email.to_string(),
                name: name.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .context("register personal request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("registration failed"));
        }
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }

        resp.json::<AuthSession>()
            .await
            .context("invalid register personal response")
    }

    pub async fn signin_api_key(&self, api_key: &str) -> Result<AuthSession> {
        let resp = self
            .client
            .post(self.url("/auth/token"))
            .header("X-API-Key", api_key)
            .send()
            .await
            .context("api key signin request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("invalid api key"));
        }
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }

        resp.json::<AuthSession>()
            .await
            .context("invalid api key signin response")
    }

    pub async fn signout(&self) -> Result<()> {
        let mut req = self.client.post(self.url("/auth/signout"));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await.context("signout request failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
    }

    pub async fn whoami(&self) -> Result<AuthSession> {
        let mut req = self.client.get(self.url("/auth/me"));
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await.context("whoami request failed")?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("not authenticated — run `companion signin`"));
        }
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<AuthSession>().await.context("invalid whoami response")
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let resp = self
            .client
            .get(self.url("/sessions"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list sessions failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<Session>>().await.context("invalid sessions response")
    }

    pub async fn create_session(&self, title: &str, mode: &str) -> Result<Session> {
        let resp = self
            .client
            .post(self.url("/sessions"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(&SessionCreateRequest {
                title: title.to_string(),
                mode: mode.to_string(),
            })
            .send()
            .await
            .context("create session failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Session>().await.context("invalid session response")
    }

    pub async fn get_session(&self, id: &str) -> Result<Session> {
        let resp = self
            .client
            .get(self.url(&format!("/sessions/{}", id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("get session failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Session>().await.context("invalid session response")
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/sessions/{}", id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("delete session failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let resp = self
            .client
            .get(self.url(&format!("/sessions/{}/messages", session_id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list messages failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<Message>>().await.context("invalid messages response")
    }

    pub async fn send_message(&self, session_id: &str, text: &str) -> Result<Message> {
        let msg = MessageCreateRequest {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: vec![ContentPart {
                part_type: "text".to_string(),
                text: Some(text.to_string()),
            }],
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        let resp = self
            .client
            .post(self.url(&format!("/sessions/{}/messages", session_id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(&msg)
            .send()
            .await
            .context("send message failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Message>().await.context("invalid message response")
    }

    pub async fn knowledge_search(&self, query: &str, limit: u32) -> Result<Vec<KnowledgeSearchResult>> {
        let resp = self
            .client
            .post(self.url("/knowledge/search"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(&KnowledgeSearchRequest {
                query: query.to_string(),
                limit,
            })
            .send()
            .await
            .context("knowledge search failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<KnowledgeSearchResult>>()
            .await
            .context("invalid search response")
    }

    pub async fn upload_document(&self, path: &Path) -> Result<UploadResponse> {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid file path"))?
            .to_string_lossy()
            .to_string();
        let file_bytes = std::fs::read(path).context("read file")?;
        let part = multipart::Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str("application/pdf")
            .unwrap();
        let form = multipart::Form::new().part("file", part);

        let resp = self
            .client
            .post(self.url("/knowledge/documents"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .multipart(form)
            .send()
            .await
            .context("upload document failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<UploadResponse>().await.context("invalid upload response")
    }

    pub async fn list_documents(&self) -> Result<Vec<serde_json::Value>> {
        let resp = self
            .client
            .get(self.url("/knowledge/documents"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list documents failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<serde_json::Value>>()
            .await
            .context("invalid documents response")
    }

    pub async fn delete_document(&self, id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/knowledge/documents/{}", id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("delete document failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
    }

    pub async fn list_meetings(&self) -> Result<Vec<Meeting>> {
        let resp = self
            .client
            .get(self.url("/meetings"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list meetings failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<Meeting>>().await.context("invalid meetings response")
    }

    pub async fn ingest_meeting(&self, req: &MeetingIngestRequest) -> Result<Meeting> {
        let resp = self
            .client
            .post(self.url("/meetings"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(req)
            .send()
            .await
            .context("ingest meeting failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Meeting>().await.context("invalid meeting response")
    }

    pub async fn usage_summary(&self) -> Result<Vec<UsageSummary>> {
        let resp = self
            .client
            .get(self.url("/usage/summary"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("usage summary failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<UsageSummary>>()
            .await
            .context("invalid usage response")
    }

    pub async fn list_api_keys(&self, user_id: &str) -> Result<Vec<ApiKeyOut>> {
        let resp = self
            .client
            .get(self.url(&format!("/company/apikeys/{}", user_id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list api keys failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<ApiKeyOut>>()
            .await
            .context("invalid api keys response")
    }

    pub async fn set_api_key(&self, provider: &str, key: &str) -> Result<ApiKeyOut> {
        let resp = self
            .client
            .post(self.url("/company/apikeys"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(&ApiKeySet {
                provider: provider.to_string(),
                plain_key: key.to_string(),
            })
            .send()
            .await
            .context("set api key failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<ApiKeyOut>().await.context("invalid api key response")
    }

    pub async fn delete_api_key(&self, key_id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/company/apikeys/key/{}", key_id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("delete api key failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
    }

    // ─── Company Auth Keys (CLI API keys) ──────────────────────────────────

    pub async fn create_auth_key(&self, name: &str) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(self.url("/company/auth-keys"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .context("create auth key failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<serde_json::Value>().await.context("invalid auth key response")
    }

    pub async fn list_auth_keys(&self) -> Result<Vec<CompanyAuthKeyOut>> {
        let resp = self
            .client
            .get(self.url("/company/auth-keys"))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("list auth keys failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<Vec<CompanyAuthKeyOut>>()
            .await
            .context("invalid auth keys response")
    }

    pub async fn delete_auth_key(&self, key_id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/company/auth-keys/{}", key_id)))
            .header("Authorization", self.auth_header().unwrap_or_default())
            .send()
            .await
            .context("delete auth key failed")?;
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
    }
}