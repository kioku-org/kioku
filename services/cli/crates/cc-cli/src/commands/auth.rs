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
            company_id: session.company_id,
            role: session.role,
        }
        .save()?;
        println!("Signed in via API key.");
        return Ok(());
    }

    // OAuth flow: show provider selector → open browser → wait for callback
    let provider = signin::select_provider()?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let state = uuid::Uuid::new_v4().to_string();

    let dashboard_url = config::resolve_dashboard_url(&base_url);
    let auth_url = format!(
        "{}/cli-auth?port={}&state={}&provider={}",
        dashboard_url,
        port,
        state,
        provider.id()
    );

    println!("Opening browser…");
    if webbrowser::open(&auth_url).is_err() {
        println!("Couldn't open browser automatically. Open this URL manually:");
        println!("  {}", auth_url);
    }
    println!("Waiting for sign-in (2 min timeout)…");

    let result = signin::wait_for_callback(listener, &state).await?;

    AuthFile {
        server_url: base_url,
        token: result.token,
        user_id: result.user_id,
        email: result.email.clone(),
        name: result.name.clone(),
        company_id: result.company_id,
        role: result.role,
    }
    .save()?;

    println!("Signed in as {} ({})", result.name, result.email);
    Ok(())
}

pub fn signout() -> Result<()> {
    AuthFile::delete()?;
    println!("Signed out.");
    Ok(())
}

pub async fn whoami() -> Result<()> {
    let auth = require_auth()?;
    let client = crate::session::make_client(&auth);
    let me = client.whoami().await?;
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
    company_name: Option<String>,
    company_slug: Option<String>,
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
    let company_name = prompt_or(company_name, "Company name: ")?;
    let email = prompt_or(email, "Email: ")?;
    let name = prompt_or(name, "Name: ")?;
    let password = prompt_or(password, "Password: ")?;

    let session = client
        .register_admin(
            &company_name,
            company_slug.as_deref(),
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
        company_id: session.company_id,
        role: session.role,
    }
    .save()?;
    println!("Registered admin and signed in.");
    Ok(())
}
