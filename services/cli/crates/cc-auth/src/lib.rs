use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn config_path(file_name: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kioku")
        .join(file_name)
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(value))
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn delete_file(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub server_url: String,
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    /// The token's default workspace (workspace) — used when
    /// `active_workspace_id` hasn't been set via `kioku ws <name>`.
    pub workspace_id: String,
    pub role: String,
    /// Workspace selected via `kioku ws <name>`, sent as the X-Workspace-Id
    /// header on every request. `#[serde(default)]` so auth.json files
    /// written before multi-workspace support still load fine.
    #[serde(default)]
    pub active_workspace_id: Option<String>,
}

impl AuthFile {
    pub fn path() -> PathBuf {
        config_path("auth.json")
    }

    pub fn load() -> Result<Option<Self>> {
        load_json(&Self::path())
    }

    pub fn save(&self) -> Result<()> {
        save_json(&Self::path(), self)
    }

    pub fn delete() -> Result<()> {
        delete_file(&Self::path())
    }
}

/// Google Calendar OAuth token — separate from the main Hivemind `AuthFile`.
/// Obtained via `kioku cal`, which transparently runs the Google consent
/// flow the first time Calendar access is needed (see
/// `ensure_valid_token_or_connect` in the CLI), distinct from the
/// dashboard-mediated main signin flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalendarAuth {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix ms when `access_token` expires.
    pub expires_at: i64,
}

impl GoogleCalendarAuth {
    pub fn path() -> PathBuf {
        config_path("google-calendar.json")
    }

    pub fn load() -> Result<Option<Self>> {
        load_json(&Self::path())
    }

    pub fn save(&self) -> Result<()> {
        save_json(&Self::path(), self)
    }

    pub fn delete() -> Result<()> {
        delete_file(&Self::path())
    }

    pub fn is_expired(&self) -> bool {
        // 60s safety margin before the real expiry.
        crate::now_ms() >= self.expires_at - 60_000
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
