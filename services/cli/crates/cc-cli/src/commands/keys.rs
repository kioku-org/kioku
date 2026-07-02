use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;

pub fn resolve_auth_key_delete_target(
    keys: &[cc_kioku::WorkspaceApiKeyOut],
    key_prefix_or_id: &str,
) -> Result<String> {
    if let Some(key) = keys.iter().find(|key| key.id == key_prefix_or_id) {
        return Ok(key.id.clone());
    }

    if let Some(key) = keys.iter().find(|key| key.key_prefix == key_prefix_or_id) {
        return Ok(key.id.clone());
    }

    Err(anyhow::anyhow!(
        "auth key `{}` not found — run `kioku keys` to inspect valid ids and prefixes",
        key_prefix_or_id
    ))
}

pub async fn run(
    ctx: AppContext,
    create: bool,
    name: String,
    delete: Option<String>,
) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if create {
        let key = client.create_auth_key(&name).await?;
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&key)?);
            return Ok(());
        }
        let raw = key
            .get("key")
            .and_then(|v| v.as_str())
            .or_else(|| key.get("raw_key").and_then(|v| v.as_str()));
        if let Some(raw_key) = raw {
            println!("API key (save this — it won't be shown again):");
            println!();
            println!("  {}", raw_key);
            println!();
            println!("Use it with:  kioku signin --api-key <key>");
        } else {
            println!("{}", serde_json::to_string_pretty(&key)?);
        }
        return Ok(());
    }

    if let Some(key_prefix) = delete {
        let keys = client.list_auth_keys().await?;
        let key_id = resolve_auth_key_delete_target(&keys, &key_prefix)?;
        client.delete_auth_key(&key_id).await?;
        println!("Deleted key {key_prefix}");
        return Ok(());
    }

    let keys = client.list_auth_keys().await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&keys)?);
        return Ok(());
    }
    if keys.is_empty() {
        println!("No API keys.");
    }
    for k in &keys {
        let last_used = k
            .last_used_at
            .and_then(|ts| chrono::DateTime::from_timestamp(ts / 1000, 0))
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "never".to_string());
        println!(
            "{} — {} (prefix: {}  last used: {})",
            k.id, k.name, k.key_prefix, last_used
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_auth_key_delete_target_accepts_prefix() {
        let fixture = vec![cc_kioku::WorkspaceApiKeyOut {
            id: "key-uuid-1".to_string(),
            user_id: "user-1".to_string(),
            name: "cli-key".to_string(),
            key_prefix: "cmp_12345678".to_string(),
            created_at: 1,
            last_used_at: None,
        }];

        let actual = resolve_auth_key_delete_target(&fixture, "cmp_12345678").unwrap();
        let expected = "key-uuid-1".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_auth_key_delete_target_accepts_id() {
        let fixture = vec![cc_kioku::WorkspaceApiKeyOut {
            id: "key-uuid-1".to_string(),
            user_id: "user-1".to_string(),
            name: "cli-key".to_string(),
            key_prefix: "cmp_12345678".to_string(),
            created_at: 1,
            last_used_at: None,
        }];

        let actual = resolve_auth_key_delete_target(&fixture, "key-uuid-1").unwrap();
        let expected = "key-uuid-1".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_auth_key_delete_target_errors_for_unknown_value() {
        let fixture = vec![cc_kioku::WorkspaceApiKeyOut {
            id: "key-uuid-1".to_string(),
            user_id: "user-1".to_string(),
            name: "cli-key".to_string(),
            key_prefix: "cmp_12345678".to_string(),
            created_at: 1,
            last_used_at: None,
        }];

        let actual = resolve_auth_key_delete_target(&fixture, "cmp_missing")
            .unwrap_err()
            .to_string();
        let expected =
            "auth key `cmp_missing` not found — run `kioku keys` to inspect valid ids and prefixes"
                .to_string();

        assert_eq!(actual, expected);
    }
}
