use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;

pub async fn run(ctx: AppContext, email: Option<String>, revoke: Option<String>) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if let Some(invite_id) = revoke {
        client.delete_invite(&invite_id).await?;
        println!("Revoked invite {invite_id}");
        return Ok(());
    }

    if let Some(email) = email {
        let invite = client.create_invite(&email, "member").await?;
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&invite)?);
            return Ok(());
        }
        // Best-effort slug lookup so the message names the workspace the
        // invitee has to type; falls back to a generic hint.
        let active_id = auth.active_workspace_id.as_deref().unwrap_or(&auth.workspace_id);
        let slug = client
            .list_workspaces()
            .await
            .ok()
            .and_then(|ws| ws.into_iter().find(|w| w.id == active_id).map(|w| w.slug))
            .unwrap_or_else(|| "<workspace>".to_string());
        println!(
            "Invited {} — existing accounts join with `kioku ws --join {}`; new users sign up to workspace `{}`.",
            invite.email, slug, slug
        );
        return Ok(());
    }

    let invites = client.list_invites().await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&invites)?);
        return Ok(());
    }
    if invites.is_empty() {
        println!("No pending invites.");
    }
    for i in &invites {
        let status = if i.used_at.is_some() {
            "accepted"
        } else {
            "pending"
        };
        println!("{} — {} ({}, {})", i.id, i.email, i.role, status);
    }
    Ok(())
}
