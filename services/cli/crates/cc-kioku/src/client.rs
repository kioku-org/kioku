use crate::types::*;
use anyhow::{Context, Result};
use reqwest::{multipart, Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::path::Path;

/// Matches the extension list hivemind's document handler recognizes
/// (services/hivemind/src/handlers/knowledge.rs) — kept in sync manually.
fn document_mime_type(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".docx") {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    } else if lower.ends_with(".pptx") {
        "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    } else if lower.ends_with(".md") {
        "text/markdown"
    } else {
        "text/plain"
    }
}

pub struct KiokuClient {
    base_url: String,
    client: Client,
    token: Option<String>,
    workspace_id: Option<String>,
}

impl KiokuClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
            token: None,
            workspace_id: None,
        }
    }

    pub fn with_token(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
            token: Some(token.to_string()),
            workspace_id: None,
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Scope every subsequent request to a specific workspace (sent as the
    /// X-Workspace-Id header), for accounts belonging to more than one.
    pub fn set_workspace_id(&mut self, workspace_id: Option<String>) {
        self.workspace_id = workspace_id;
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

    /// Attach the `Authorization` header when a token is set, and the
    /// `X-Workspace-Id` header when a non-default workspace is active;
    /// leave the request untouched otherwise.
    fn authed(&self, req: RequestBuilder) -> RequestBuilder {
        let req = match self.auth_header() {
            Some(auth) => req.header("Authorization", auth),
            None => req,
        };
        match &self.workspace_id {
            Some(id) => req.header("X-Workspace-Id", id),
            None => req,
        }
    }

    async fn handle_error(resp: Response) -> anyhow::Error {
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

    /// Decode a successful response as JSON, or turn a non-2xx response into
    /// an `Err` describing the server's error message.
    async fn parse<T: DeserializeOwned>(resp: Response, err_context: &str) -> Result<T> {
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        resp.json::<T>()
            .await
            .with_context(|| err_context.to_string())
    }

    /// Assert a successful response, discarding the body.
    async fn expect_ok(resp: Response) -> Result<()> {
        if !resp.status().is_success() {
            return Err(Self::handle_error(resp).await);
        }
        Ok(())
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
        Self::parse(resp, "invalid signin response").await
    }

    pub async fn register_personal(
        &self,
        email: &str,
        name: &str,
        password: &str,
    ) -> Result<AuthSession> {
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
        Self::parse(resp, "invalid register personal response").await
    }

    pub async fn register_admin(
        &self,
        workspace_name: &str,
        workspace_slug: Option<&str>,
        email: &str,
        name: &str,
        password: &str,
    ) -> Result<AuthSession> {
        let resp = self
            .client
            .post(self.url("/auth/register/admin"))
            .json(&RegisterAdminRequest {
                workspace_name: workspace_name.to_string(),
                workspace_slug: workspace_slug.map(|slug| slug.to_string()),
                email: email.to_string(),
                name: name.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .context("register admin request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("admin registration failed"));
        }
        Self::parse(resp, "invalid register admin response").await
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
        Self::parse(resp, "invalid api key signin response").await
    }

    pub async fn signout(&self) -> Result<()> {
        let resp = self
            .authed(self.client.post(self.url("/auth/signout")))
            .send()
            .await
            .context("signout request failed")?;
        Self::expect_ok(resp).await
    }

    /// The caller's Vexa gateway API key (X-API-Key), minted/resolved server-side.
    /// Used for endpoints only the gateway serves, e.g. live transcripts.
    pub async fn get_vexa_token(&self) -> Result<String> {
        let resp = self
            .authed(self.client.get(self.url("/vexa/token")))
            .send()
            .await
            .context("vexa token request failed")?;
        let body: serde_json::Value = Self::parse(resp, "invalid vexa token response").await?;
        body.get("vexa_api_key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("no vexa api key available for this account"))
    }

    pub async fn whoami(&self) -> Result<AuthSession> {
        let resp = self
            .authed(self.client.get(self.url("/auth/me")))
            .send()
            .await
            .context("whoami request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!("not authenticated — run `kioku signin`"));
        }
        Self::parse(resp, "invalid whoami response").await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let resp = self
            .authed(self.client.get(self.url("/sessions")))
            .send()
            .await
            .context("list sessions failed")?;
        Self::parse(resp, "invalid sessions response").await
    }

    pub async fn create_session(&self, title: &str, mode: &str) -> Result<Session> {
        let resp = self
            .authed(self.client.post(self.url("/sessions")))
            .json(&SessionCreateRequest {
                title: title.to_string(),
                mode: mode.to_string(),
            })
            .send()
            .await
            .context("create session failed")?;
        Self::parse(resp, "invalid session response").await
    }

    pub async fn get_session(&self, id: &str) -> Result<Session> {
        let resp = self
            .authed(self.client.get(self.url(&format!("/sessions/{}", id))))
            .send()
            .await
            .context("get session failed")?;
        Self::parse(resp, "invalid session response").await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        let resp = self
            .authed(self.client.delete(self.url(&format!("/sessions/{}", id))))
            .send()
            .await
            .context("delete session failed")?;
        Self::expect_ok(resp).await
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let resp = self
            .authed(
                self.client
                    .get(self.url(&format!("/sessions/{}/messages", session_id))),
            )
            .send()
            .await
            .context("list messages failed")?;
        Self::parse(resp, "invalid messages response").await
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
            .authed(
                self.client
                    .post(self.url(&format!("/sessions/{}/messages", session_id))),
            )
            .json(&msg)
            .send()
            .await
            .context("send message failed")?;
        Self::parse(resp, "invalid message response").await
    }

    pub async fn knowledge_search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let resp = self
            .authed(self.client.post(self.url("/knowledge/search")))
            .json(&KnowledgeSearchRequest {
                query: query.to_string(),
                limit,
            })
            .send()
            .await
            .context("knowledge search failed")?;
        Self::parse(resp, "invalid search response").await
    }

    pub async fn upload_document(&self, path: &Path) -> Result<UploadResponse> {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid file path"))?
            .to_string_lossy()
            .to_string();
        let file_bytes = std::fs::read(path).context("read file")?;
        let mime = document_mime_type(&filename);
        let part = multipart::Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str(mime)
            .unwrap();
        let form = multipart::Form::new().part("file", part);

        let resp = self
            .authed(self.client.post(self.url("/knowledge/documents")))
            .multipart(form)
            .send()
            .await
            .context("upload document failed")?;
        Self::parse(resp, "invalid upload response").await
    }

    pub async fn list_documents(&self) -> Result<Vec<serde_json::Value>> {
        let resp = self
            .authed(self.client.get(self.url("/knowledge/documents")))
            .send()
            .await
            .context("list documents failed")?;
        Self::parse(resp, "invalid documents response").await
    }

    pub async fn delete_document(&self, id: &str) -> Result<()> {
        let resp = self
            .authed(
                self.client
                    .delete(self.url(&format!("/knowledge/documents/{}", id))),
            )
            .send()
            .await
            .context("delete document failed")?;
        Self::expect_ok(resp).await
    }

    pub async fn list_meetings(&self) -> Result<Vec<Meeting>> {
        let resp = self
            .authed(self.client.get(self.url("/meetings")))
            .send()
            .await
            .context("list meetings failed")?;
        Self::parse(resp, "invalid meetings response").await
    }

    pub async fn get_meeting(&self, meeting_id: &str) -> Result<Meeting> {
        let resp = self
            .authed(
                self.client
                    .get(self.url(&format!("/meetings/{meeting_id}"))),
            )
            .send()
            .await
            .context("get meeting failed")?;
        Self::parse(resp, "invalid meeting response").await
    }

    pub async fn get_transcript(&self, meeting_id: &str) -> Result<Vec<serde_json::Value>> {
        let resp = self
            .authed(
                self.client
                    .get(self.url(&format!("/meetings/{meeting_id}/transcript"))),
            )
            .send()
            .await
            .context("get transcript failed")?;
        Self::parse(resp, "invalid transcript response").await
    }

    /// Join a meeting given its link (Google Meet, Teams, or Zoom — platform
    /// is auto-detected server-side from the URL).
    pub async fn join_meeting(&self, meeting_url: &str) -> Result<serde_json::Value> {
        let resp = self
            .authed(self.client.post(self.url("/vexa/bots")))
            .json(&serde_json::json!({ "meeting_url": meeting_url }))
            .send()
            .await
            .context("join meeting failed")?;
        Self::parse(resp, "invalid join meeting response").await
    }

    pub async fn list_bot_status(&self) -> Result<Vec<BotStatus>> {
        let resp = self
            .authed(self.client.get(self.url("/vexa/bots/status")))
            .send()
            .await
            .context("list bot status failed")?;
        Self::parse::<BotStatusResponse>(resp, "invalid bot status response")
            .await
            .map(|r| r.running_bots)
    }

    pub async fn stop_bot(&self, platform: &str, native_meeting_id: &str) -> Result<()> {
        let resp = self
            .authed(
                self.client
                    .delete(self.url(&format!("/vexa/bots/{platform}/{native_meeting_id}"))),
            )
            .send()
            .await
            .context("stop bot failed")?;
        Self::expect_ok(resp).await
    }

    pub async fn ingest_meeting(&self, req: &MeetingIngestRequest) -> Result<Meeting> {
        let resp = self
            .authed(self.client.post(self.url("/meetings")))
            .json(req)
            .send()
            .await
            .context("ingest meeting failed")?;
        Self::parse(resp, "invalid meeting response").await
    }

    // ─── Workspace Auth Keys (CLI API keys) ──────────────────────────────────

    pub async fn create_auth_key(&self, name: &str) -> Result<serde_json::Value> {
        let resp = self
            .authed(self.client.post(self.url("/workspace/auth-keys")))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .context("create auth key failed")?;
        Self::parse(resp, "invalid auth key response").await
    }

    pub async fn list_auth_keys(&self) -> Result<Vec<WorkspaceApiKeyOut>> {
        let resp = self
            .authed(self.client.get(self.url("/workspace/auth-keys")))
            .send()
            .await
            .context("list auth keys failed")?;
        Self::parse(resp, "invalid auth keys response").await
    }

    pub async fn delete_auth_key(&self, key_id: &str) -> Result<()> {
        let resp = self
            .authed(
                self.client
                    .delete(self.url(&format!("/workspace/auth-keys/{}", key_id))),
            )
            .send()
            .await
            .context("delete auth key failed")?;
        Self::expect_ok(resp).await
    }

    // ─── Workspace Invites (Pro/Teams teammate sharing) ──────────────────────

    pub async fn create_invite(&self, email: &str, role: &str) -> Result<InviteOut> {
        let resp = self
            .authed(self.client.post(self.url("/workspace/invites")))
            .json(&InviteCreateRequest {
                email: email.to_string(),
                role: role.to_string(),
            })
            .send()
            .await
            .context("create invite failed")?;
        Self::parse(resp, "invalid invite response").await
    }

    pub async fn list_invites(&self) -> Result<Vec<InviteOut>> {
        let resp = self
            .authed(self.client.get(self.url("/workspace/invites")))
            .send()
            .await
            .context("list invites failed")?;
        Self::parse(resp, "invalid invites response").await
    }

    pub async fn delete_invite(&self, invite_id: &str) -> Result<()> {
        let resp = self
            .authed(
                self.client
                    .delete(self.url(&format!("/workspace/invites/{}", invite_id))),
            )
            .send()
            .await
            .context("delete invite failed")?;
        Self::expect_ok(resp).await
    }

    // ─── Workspaces (kioku ws) ───────────────────────────────────────────────

    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceOut>> {
        let resp = self
            .authed(self.client.get(self.url("/workspaces")))
            .send()
            .await
            .context("list workspaces failed")?;
        Self::parse(resp, "invalid workspaces response").await
    }

    pub async fn create_workspace(
        &self,
        name: &str,
        slug: Option<&str>,
    ) -> Result<WorkspaceCreateResponse> {
        let resp = self
            .authed(self.client.post(self.url("/workspaces")))
            .json(&WorkspaceCreateRequest {
                name: name.to_string(),
                slug: slug.map(|s| s.to_string()),
            })
            .send()
            .await
            .context("create workspace failed")?;
        Self::parse(resp, "invalid workspace response").await
    }

    /// Invite a teammate into a specific workspace by id or slug — unlike
    /// `create_invite`, which always targets the active workspace, this can
    /// target any workspace the caller is an admin of.
    pub async fn create_workspace_invite(
        &self,
        workspace_id_or_slug: &str,
        email: &str,
        role: &str,
    ) -> Result<InviteOut> {
        let resp = self
            .authed(
                self.client
                    .post(self.url(&format!("/workspaces/{}/invites", workspace_id_or_slug))),
            )
            .json(&InviteCreateRequest {
                email: email.to_string(),
                role: role.to_string(),
            })
            .send()
            .await
            .context("create workspace invite failed")?;
        Self::parse(resp, "invalid invite response").await
    }
}

#[cfg(test)]
mod tests {
    use super::document_mime_type;

    #[test]
    fn document_mime_type_matches_extension() {
        assert_eq!(document_mime_type("report.pdf"), "application/pdf");
        assert_eq!(
            document_mime_type("REPORT.PDF"),
            "application/pdf",
            "extension match should be case-insensitive"
        );
        assert_eq!(
            document_mime_type("notes.docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(
            document_mime_type("slides.pptx"),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        );
        assert_eq!(document_mime_type("readme.md"), "text/markdown");
        assert_eq!(document_mime_type("plain.txt"), "text/plain");
        assert_eq!(
            document_mime_type("no_extension"),
            "text/plain",
            "unknown/missing extension should fall back to text/plain, not a wrong-but-specific type"
        );
    }
}
