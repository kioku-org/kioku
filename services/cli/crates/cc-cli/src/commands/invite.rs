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
        println!(
            "Invited {} — they can now sign up to join your workspace.",
            invite.email
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
