use anyhow::Result;
use cc_auth::AuthFile;
use cc_kioku::KiokuClient;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_subscriber::{fmt, EnvFilter};

mod signin;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "kioku-org/kioku";
const DEFAULT_SERVER_URL: &str = "https://api.kioku.chat";
const DEFAULT_DASHBOARD_URL: &str = "https://dashboard.kioku.chat";

#[derive(Parser, Debug)]
#[command(
    name = "kioku",
    version,
    about = "Kioku — context infrastructure client"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short = 'C', long, global = true)]
    cwd: Option<PathBuf>,

    #[arg(long, global = true)]
    server: Option<String>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    #[command(about = "Register the initial admin account for a self-hosted server", hide = true)]
    RegisterAdmin {
        #[arg(long)]
        company_name: Option<String>,
        #[arg(long)]
        company_slug: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
    #[command(about = "Sign in with Google, Github or API key")]
    Signin {
        #[arg(long)]
        api_key: Option<String>,
    },
    #[command(about = "Sign out and clear stored credentials")]
    Signout,
    #[command(about = "Show current user info")]
    Whoami,
    #[command(about = "Print stored auth token")]
    AuthToken,
    #[command(about = "List sessions")]
    SessionsList,
    #[command(about = "Create a new session")]
    SessionsCreate {
        #[arg(long)]
        title: Option<String>,
    },
    #[command(about = "Get session details")]
    SessionsGet { session_id: String },
    #[command(about = "Delete a session")]
    SessionsDelete { session_id: String },
    #[command(about = "Send a message to a session")]
    Send {
        session_id: String,
        message: Vec<String>,
    },
    #[command(about = "List messages in a session")]
    Messages { session_id: String },
    #[command(about = "Search knowledge base")]
    KnowledgeSearch { query: String },
    #[command(about = "Upload a PDF document")]
    KnowledgeUpload { file: String },
    #[command(about = "List uploaded documents")]
    KnowledgeDocuments,
    #[command(about = "Delete a document")]
    KnowledgeDelete { document_id: String },
    #[command(about = "List meetings")]
    MeetingsList,
    #[command(about = "Create a long-lived API key for CLI auth")]
    AuthKeyCreate {
        #[arg(long, default_value = "cli-key")]
        name: String,
    },
    #[command(about = "List long-lived API keys")]
    AuthKeyList,
    #[command(about = "Delete a long-lived API key")]
    AuthKeyDelete {
        #[arg(value_name = "KEY_PREFIX_OR_ID")]
        key_prefix: String,
    },
    #[command(about = "Print MCP server configuration for AI clients")]
    Mcp,
    #[command(about = "Check for updates")]
    UpgradeCheck,
    #[command(about = "Upgrade to the latest version")]
    Upgrade,
}

fn require_auth() -> Result<AuthFile> {
    AuthFile::load()?.ok_or_else(|| anyhow::anyhow!("not signed in — run `kioku signin`"))
}

fn make_client(auth: &AuthFile) -> KiokuClient {
    KiokuClient::with_token(&auth.server_url, &auth.token)
}

fn resolve_server_url(server_override: Option<&str>) -> String {
    resolve_server_url_from(
        server_override,
        std::env::var("KIOKU_SERVER").ok().as_deref(),
    )
}

fn resolve_dashboard_url(server_url: &str) -> String {
    if let Ok(v) = std::env::var("KIOKU_DASHBOARD") {
        return v;
    }
    if server_url.contains("api.kioku.chat") {
        return DEFAULT_DASHBOARD_URL.to_string();
    }
    // Local dev: API is typically :9100, dashboard :3001
    if let Some(stripped) = server_url
        .strip_prefix("http://localhost:")
        .or_else(|| server_url.strip_prefix("http://127.0.0.1:"))
    {
        let prefix = &server_url[..server_url.len() - stripped.len()];
        return format!("{}3001", prefix);
    }
    DEFAULT_DASHBOARD_URL.to_string()
}

fn resolve_server_url_from(server_override: Option<&str>, env_server: Option<&str>) -> String {
    server_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| env_server.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(DEFAULT_SERVER_URL)
        .to_string()
}

fn prompt_or(value: Option<String>, label: &str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Ok(rprompt::prompt_reply(label)?.trim().to_string()),
    }
}

fn meeting_mcp_url(server_url: &str) -> String {
    // Derive the meeting MCP URL from the Hivemind server URL.
    // For local/Docker deployments, replace the port with 18888.
    // For hostnames without a port (e.g. https://api.kioku.chat), the operator
    // exposes the meeting MCP at a separate hostname (e.g. mcp.kioku.chat).
    let trimmed = server_url.trim_end_matches('/');
    if let Some(pos) = trimmed.rfind(':') {
        let after_colon = &trimmed[pos + 1..];
        // Only substitute if it looks like a port number
        if after_colon.chars().all(|c| c.is_ascii_digit()) {
            let prefix = &trimmed[..pos + 1];
            return format!("{}18888/mcp", prefix);
        }
    }
    // No port: can't derive automatically, show a placeholder
    format!("{}/meeting-mcp/mcp", trimmed)
}

fn mcp_config_json(server_url: &str, token: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "Kioku": {
                "url": format!("{}/mcp", server_url.trim_end_matches('/')),
                "headers": {
                    "Authorization": format!("Bearer {}", token)
                }
            },
            "Kioku Meetings": {
                "url": meeting_mcp_url(server_url),
                "headers": {
                    "Authorization": format!("Bearer {}", token)
                }
            }
        }
    }))
    .expect("json serialization is infallible")
}

fn resolve_auth_key_delete_target(
    keys: &[cc_kioku::CompanyAuthKeyOut],
    key_prefix_or_id: &str,
) -> Result<String> {
    if let Some(key) = keys.iter().find(|key| key.id == key_prefix_or_id) {
        return Ok(key.id.clone());
    }

    if let Some(key) = keys.iter().find(|key| key.key_prefix == key_prefix_or_id) {
        return Ok(key.id.clone());
    }

    Err(anyhow::anyhow!(
        "auth key `{}` not found — run `kioku auth-key-list` to inspect valid ids and prefixes",
        key_prefix_or_id
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        None => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "kioku — context infrastructure client")?;
            writeln!(stdout, "Run `kioku help` for available commands.")?;
            Ok(())
        }
        Some(cmd) => run(cmd, cli.server).await,
    }
}

async fn run(cmd: Commands, server: Option<String>) -> Result<()> {
    match cmd {
        Commands::RegisterAdmin {
            company_name,
            company_slug,
            email,
            name,
            password,
        } => {
            let base_url = resolve_server_url(server.as_deref());
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

            let auth = AuthFile {
                server_url: base_url,
                token: session.token,
                user_id: session.user_id,
                email: session.email,
                name: session.name,
                company_id: session.company_id,
                role: session.role,
            };
            auth.save()?;
            println!("Registered admin and signed in.");
        }
        Commands::Signin { api_key } => {
            let base_url = resolve_server_url(server.as_deref());
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
            } else {
                // OAuth flow: show provider selector → open browser → wait for callback
                let provider = signin::select_provider()?;

                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
                let port = listener.local_addr()?.port();
                let state = uuid::Uuid::new_v4().to_string();

                let dashboard_url = resolve_dashboard_url(&base_url);
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
            }
        }
        Commands::Signout => {
            AuthFile::delete()?;
            println!("Signed out.");
        }
        Commands::Whoami => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let me = client.whoami().await?;
            println!("name:    {}", me.name);
            println!("email:   {}", me.email);
            println!("role:    {}", me.role);
            println!("server:  {}", auth.server_url);
        }
        Commands::AuthToken => {
            let auth = require_auth()?;
            println!("{}", auth.token);
        }
        Commands::SessionsList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let sessions = client.list_sessions().await?;
            if sessions.is_empty() {
                println!("No sessions.");
            }
            for s in &sessions {
                println!("{} — {}", s.id, s.title);
            }
        }
        Commands::SessionsCreate { title } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let session = client
                .create_session(title.as_deref().unwrap_or("New session"), "research")
                .await?;
            println!("Created session: {}", session.id);
        }
        Commands::SessionsGet { session_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let session = client.get_session(&session_id).await?;
            println!("id:    {}", session.id);
            println!("title: {}", session.title);
        }
        Commands::SessionsDelete { session_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.delete_session(&session_id).await?;
            println!("Deleted session {session_id}");
        }
        Commands::Send {
            session_id,
            message,
        } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let msg = message.join(" ");
            let resp = client.send_message(&session_id, &msg).await?;
            for part in &resp.content {
                if let Some(text) = &part.text {
                    println!("{text}");
                }
            }
        }
        Commands::Messages { session_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let msgs = client.list_messages(&session_id).await?;
            if msgs.is_empty() {
                println!("No messages.");
            }
            for m in &msgs {
                let text: String = m
                    .content
                    .iter()
                    .filter_map(|p| p.text.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                let preview: String = text.chars().take(200).collect();
                let ellipsis = if text.len() > 200 { "…" } else { "" };
                println!("[{}] {}{}", m.role.to_uppercase(), preview, ellipsis);
                println!();
            }
        }
        Commands::KnowledgeSearch { query } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let results = client.knowledge_search(&query, 5).await?;
            if results.is_empty() {
                println!("No results.");
            }
            for (i, r) in results.iter().enumerate() {
                let text = r.chunk.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let preview: String = text.chars().take(300).collect();
                let ellipsis = if text.len() > 300 { "…" } else { "" };
                println!("{}. [score {:.2}]  {}{}", i + 1, r.score, preview, ellipsis);
                println!();
            }
        }
        Commands::KnowledgeUpload { file } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let path = Path::new(&file);
            client.upload_document(path).await?;
            println!("Uploaded {file}");
        }
        Commands::KnowledgeDocuments => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let docs = client.list_documents().await?;
            if docs.is_empty() {
                println!("No documents.");
            }
            for d in &docs {
                let id = d.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let name = d.get("name").and_then(|v| v.as_str())
                    .or_else(|| d.get("filename").and_then(|v| v.as_str()))
                    .unwrap_or("untitled");
                println!("{} — {}", id, name);
            }
        }
        Commands::KnowledgeDelete { document_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.delete_document(&document_id).await?;
            println!("Deleted document {document_id}");
        }
        Commands::MeetingsList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let meetings = client.list_meetings().await?;
            if meetings.is_empty() {
                println!("No meetings.");
            }
            for m in &meetings {
                let date = chrono::DateTime::from_timestamp(m.date / 1000, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| m.date.to_string());
                println!("{} — {} ({})", m.id, m.title, date);
            }
        }
        Commands::AuthKeyCreate { name } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let key = client.create_auth_key(&name).await?;
            let raw = key.get("key").and_then(|v| v.as_str())
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
        }
        Commands::AuthKeyList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let keys = client.list_auth_keys().await?;
            if keys.is_empty() {
                println!("No API keys.");
            }
            for k in &keys {
                let last_used = k.last_used_at
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts / 1000, 0))
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "never".to_string());
                println!("{} — {} (prefix: {}  last used: {})", k.id, k.name, k.key_prefix, last_used);
            }
        }
        Commands::AuthKeyDelete { key_prefix } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let keys = client.list_auth_keys().await?;
            let key_id = resolve_auth_key_delete_target(&keys, &key_prefix)?;
            client.delete_auth_key(&key_id).await?;
            println!("Deleted key {key_prefix}");
        }
        Commands::Mcp => {
            let auth = require_auth()?;
            println!("{}", mcp_config_json(&auth.server_url, &auth.token));
        }
        Commands::UpgradeCheck => {
            let info = cc_upgrade::check_for_update(REPO, VERSION).await?;
            if info.latest_version == VERSION {
                println!("Up to date (v{VERSION}).");
            } else {
                println!(
                    "New version available: v{} (current: v{VERSION})",
                    info.latest_version
                );
            }
        }
        Commands::Upgrade => {
            let msg = cc_upgrade::perform_upgrade(REPO, VERSION).await?;
            println!("{msg}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        mcp_config_json, meeting_mcp_url, resolve_auth_key_delete_target, resolve_server_url_from,
        Cli, Commands, DEFAULT_SERVER_URL,
    };
    use cc_kioku::CompanyAuthKeyOut;
    use clap::Parser;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_server_url_prefers_cli_override() {
        let actual =
            resolve_server_url_from(Some("https://cli.example"), Some("https://env.example"));
        let expected = "https://cli.example".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_server_url_uses_env_when_cli_missing() {
        let actual = resolve_server_url_from(None, Some("https://env.example"));
        let expected = "https://env.example".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_server_url_defaults_to_local_hivemind() {
        let actual = resolve_server_url_from(None, None);
        let expected = DEFAULT_SERVER_URL.to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn cli_parses_register_admin_with_server_override() {
        let fixture = [
            "kioku",
            "--server",
            "http://localhost:9100",
            "register-admin",
            "--company-name",
            "Kioku",
            "--email",
            "admin@example.com",
            "--name",
            "Admin",
            "--password",
            "password123",
        ];

        let actual = Cli::parse_from(fixture);
        let expected = Some(Commands::RegisterAdmin {
            company_name: Some("Kioku".to_string()),
            company_slug: None,
            email: Some("admin@example.com".to_string()),
            name: Some("Admin".to_string()),
            password: Some("password123".to_string()),
        });

        assert_eq!(actual.command, expected);
        assert_eq!(actual.server, Some("http://localhost:9100".to_string()));
    }

    #[test]
    fn resolve_auth_key_delete_target_accepts_prefix() {
        let fixture = vec![CompanyAuthKeyOut {
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
        let fixture = vec![CompanyAuthKeyOut {
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
    fn mcp_config_json_contains_both_servers() {
        let json_str = mcp_config_json("http://localhost:9100", "test-token");
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let servers = &v["mcpServers"];
        assert_eq!(servers["Kioku"]["url"], "http://localhost:9100/mcp");
        assert_eq!(
            servers["Kioku"]["headers"]["Authorization"],
            "Bearer test-token"
        );
        assert_eq!(
            servers["Kioku Meetings"]["url"],
            "http://localhost:18888/mcp"
        );
        assert_eq!(
            servers["Kioku Meetings"]["headers"]["Authorization"],
            "Bearer test-token"
        );
    }

    #[test]
    fn meeting_mcp_url_replaces_port() {
        assert_eq!(
            meeting_mcp_url("http://localhost:9100"),
            "http://localhost:18888/mcp"
        );
        assert_eq!(
            meeting_mcp_url("http://localhost:9100/"),
            "http://localhost:18888/mcp"
        );
    }

    #[test]
    fn meeting_mcp_url_no_port_returns_placeholder() {
        let url = meeting_mcp_url("https://api.kioku.chat");
        assert!(url.contains("mcp"), "should contain mcp in URL: {url}");
    }

    #[test]
    fn resolve_auth_key_delete_target_errors_for_unknown_value() {
        let fixture = vec![CompanyAuthKeyOut {
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
            "auth key `cmp_missing` not found — run `kioku auth-key-list` to inspect valid ids and prefixes"
                .to_string();

        assert_eq!(actual, expected);
    }
}
