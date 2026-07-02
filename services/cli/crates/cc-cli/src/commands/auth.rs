use crate::config;
use crate::context::AppContext;
use crate::session::require_auth;
use crate::signin;
use anyhow::Result;
use cc_auth::AuthFile;
use cc_kioku::KiokuClient;

fn prompt_or(value: Option<String>, label: &str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Ok(rprompt::prompt_reply(label)?.trim().to_string()),
    }
}

pub async fn signin(ctx: AppContext, api_key: Option<String>) -> Result<()> {
    let base_url = ctx.base_url();

    if let Some(key) = api_key {
        let client = KiokuClient::new(&base_url);
        let session = client.signin_api_key(&key).await?;
        AuthFile {
            server_url: base_url,
            token: session.token,
            user_id: session.user_id,
            email: session.email,
            name: session.name,
            workspace_id: session.workspace_id,
            role: session.role,
            active_workspace_id: None,
        }
        .save()?;
        println!("Signed in via API key.");
        return Ok(());
    }

    // OAuth flow: show provider selector → open browser → wait for callback.
    // Picking Google requests Calendar access in the same round trip (see
    // signin::run_oauth_flow / the dashboard's `google-calendar` provider)
    // — no separate `kioku cal` connect step needed afterward. GitHub can't
    // grant Google Calendar scope, so nothing changes for that path.
    let provider = signin::select_provider()?;
    let dashboard_url = config::resolve_dashboard_url(&base_url);
    let result = signin::run_oauth_flow(&dashboard_url, &provider).await?;

    AuthFile {
        server_url: base_url,
        token: result.token,
        user_id: result.user_id,
        email: result.email.clone(),
        name: result.name.clone(),
        workspace_id: result.workspace_id,
        role: result.role,
        active_workspace_id: None,
    }
    .save()?;

    println!("Signed in as {} ({})", result.name, result.email);

    if let (Some(access_token), Some(refresh_token)) =
        (result.google_access_token, result.google_refresh_token)
    {
        crate::google_calendar::save_from_signin(
            access_token,
            refresh_token,
            result.google_token_expires_at,
        )?;
        println!("Also connected Google Calendar access (used by `kioku cal`).");
    }

    Ok(())
}

pub fn signout() -> Result<()> {
    AuthFile::delete()?;
    println!("Signed out.");
    Ok(())
}

pub async fn whoami(ctx: AppContext) -> Result<()> {
    let auth = require_auth()?;
    let client = crate::session::make_client(&auth);
    let me = client.whoami().await?;
    if ctx.json {
        // Deliberately omit `me.token` — whoami is a read-only identity
        // check, not a credential export (use `kioku --token` for that).
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "user_id": me.user_id,
                "email": me.email,
                "name": me.name,
                "workspace_id": me.workspace_id,
                "workspace_name": me.workspace_name,
                "workspace_slug": me.workspace_slug,
                "role": me.role,
                "server": auth.server_url,
            }))?
        );
        return Ok(());
    }
    println!("name:    {}", me.name);
    println!("email:   {}", me.email);
    println!("role:    {}", me.role);
    println!("server:  {}", auth.server_url);
    Ok(())
}

pub fn token() -> Result<()> {
    let auth = require_auth()?;
    println!("{}", auth.token);
    Ok(())
}

pub async fn register_admin(
    ctx: AppContext,
    workspace_name: Option<String>,
    workspace_slug: Option<String>,
    email: Option<String>,
    name: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let base_url = ctx.base_url();
    if base_url.contains("api.kioku.chat") {
        anyhow::bail!(
            "register-admin is for self-hosted servers only.\n\
             Sign in at https://dashboard.kioku.chat instead, or use `kioku signin`."
        );
    }
    let client = KiokuClient::new(&base_url);
    let workspace_name = prompt_or(workspace_name, "Workspace name: ")?;
    let email = prompt_or(email, "Email: ")?;
    let name = prompt_or(name, "Name: ")?;
    let password = prompt_or(password, "Password: ")?;

    let session = client
        .register_admin(
            &workspace_name,
            workspace_slug.as_deref(),
            &email,
            &name,
            &password,
        )
        .await?;

    AuthFile {
        server_url: base_url,
        token: session.token,
        user_id: session.user_id,
        email: session.email,
        name: session.name,
        workspace_id: session.workspace_id,
        role: session.role,
        active_workspace_id: None,
    }
    .save()?;
    println!("Registered admin and signed in.");
    Ok(())
}
