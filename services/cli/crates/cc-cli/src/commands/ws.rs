use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;

pub async fn run(
    ctx: AppContext,
    name: Option<String>,
    create: Option<String>,
    invite: Option<String>,
) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if let Some(workspace_name) = create {
        let result = client.create_workspace(&workspace_name, None).await?;
        let mut updated = auth;
        updated.token = result.token;
        updated.workspace_id = result.workspace.id.clone();
        updated.role = result.workspace.role.clone();
        updated.active_workspace_id = Some(result.workspace.id.clone());
        updated.save()?;
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&result.workspace)?);
        } else {
            println!(
                "Created workspace `{}` and switched to it.",
                result.workspace.name
            );
        }
        return Ok(());
    }

    if let Some(email) = invite {
        // clap's `requires = "name"` on --invite guarantees this is set.
        let target = name.expect("--invite requires a workspace NAME");
        let inv = client
            .create_workspace_invite(&target, &email, "member")
            .await?;
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&inv)?);
        } else {
            println!("Invited {} to workspace `{}`.", inv.email, target);
        }
        return Ok(());
    }

    if let Some(target) = name {
        let workspaces = client.list_workspaces().await?;
        let matched = workspaces
            .iter()
            .find(|w| w.id == target || w.slug == target || w.name == target)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "workspace `{}` not found — run `kioku ws` to see your workspaces",
                    target
                )
            })?;

        let mut updated = auth;
        updated.active_workspace_id = Some(matched.id.clone());
        updated.save()?;
        println!("Switched to workspace `{}`.", matched.name);
        return Ok(());
    }

    let workspaces = client.list_workspaces().await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&workspaces)?);
        return Ok(());
    }
    if workspaces.is_empty() {
        println!("No workspaces.");
    }
    let active_id = auth
        .active_workspace_id
        .as_deref()
        .unwrap_or(&auth.workspace_id);
    for w in &workspaces {
        let marker = if w.id == active_id { "*" } else { " " };
        println!(
            "{} {} — {} ({}, {})",
            marker, w.slug, w.name, w.tier, w.role
        );
    }
    Ok(())
}
