use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub server_url: String,
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub company_id: String,
    pub role: String,
}

impl AuthFile {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kioku")
            .join("auth.json")
    }

    pub fn load() -> Result<Option<Self>> {
        let path = Self::path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let auth: AuthFile = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(auth))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn delete() -> Result<()> {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(())
    }
}

/// Google Calendar OAuth token — separate from the main Hivemind `AuthFile`.
/// Obtained via a dedicated direct CLI<->Google OAuth flow (`kioku signin --calendar`),
/// distinct from the dashboard-mediated main signin flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalendarAuth {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix ms when `access_token` expires.
    pub expires_at: i64,
}

impl GoogleCalendarAuth {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kioku")
            .join("google-calendar.json")
    }

    pub fn load() -> Result<Option<Self>> {
        let path = Self::path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let auth: GoogleCalendarAuth = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(auth))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn delete() -> Result<()> {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(())
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
