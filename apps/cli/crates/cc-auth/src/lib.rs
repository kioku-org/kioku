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
            .join("companion")
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
        std::fs::write(&path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn delete() -> Result<()> {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(())
    }
}