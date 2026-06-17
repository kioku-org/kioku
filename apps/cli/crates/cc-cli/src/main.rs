use anyhow::{Context, Result};
use cc_auth::AuthFile;
use cc_kioku::KiokuClient;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_subscriber::{fmt, EnvFilter};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "kioku-org/kioku";

#[derive(Parser)]
#[command(
    name = "kioku",
    version,
    about = "Kioku CLI — context infrastructure client"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short = 'C', long, global = true)]
    cwd: Option<PathBuf>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Sign in with email/password or API key")]
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
    SessionsGet {
        session_id: String,
    },
    #[command(about = "Delete a session")]
    SessionsDelete {
        session_id: String,
    },
    #[command(about = "Send a message to a session")]
    Send {
        session_id: String,
        message: Vec<String>,
    },
    #[command(about = "List messages in a session")]
    Messages {
        session_id: String,
    },
    #[command(about = "Search knowledge base")]
    KnowledgeSearch {
        query: String,
    },
    #[command(about = "Upload a PDF document")]
    KnowledgeUpload {
        file: String,
    },
    #[command(about = "List uploaded documents")]
    KnowledgeDocuments,
    #[command(about = "Delete a document")]
    KnowledgeDelete {
        document_id: String,
    },
    #[command(about = "List meetings")]
    MeetingsList,
    #[command(about = "Show token usage summary")]
    Usage,
    #[command(about = "List API keys for current user")]
    ApikeysList,
    #[command(about = "Set an API key for a provider")]
    ApikeysSet {
        provider: String,
        key: String,
    },
    #[command(about = "Delete an API key")]
    ApikeysDelete {
        provider: String,
    },
    #[command(about = "Create a long-lived API key for CLI auth")]
    AuthKeyCreate {
        #[arg(long, default_value = "cli-key")]
        name: String,
    },
    #[command(about = "List long-lived API keys")]
    AuthKeyList,
    #[command(about = "Delete a long-lived API key")]
    AuthKeyDelete {
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
    AuthFile::load()?
        .ok_or_else(|| anyhow::anyhow!("not signed in — run `kioku signin`"))
}

fn make_client(auth: &AuthFile) -> KiokuClient {
    KiokuClient::with_token(&auth.server_url, &auth.token)
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
        Some(cmd) => run(cmd).await,
    }
}

async fn run(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Signin { api_key } => {
            let base_url = std::env::var("KIOKU_SERVER")
                .unwrap_or_else(|_| "https://api.coolcmyk.dev".to_string());
            let client = KiokuClient::new(&base_url);
            if let Some(key) = api_key {
                let session = client.signin_api_key(&key).await?;
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
                println!("Signed in via API key.");
            } else {
                let email = rprompt::prompt_reply("Email: ")?;
                let password = rprompt::prompt_reply("Password: ")?;
                let session = client.signin(&email, &password).await?;
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
                println!("Signed in.");
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
            println!("{} ({}) [{}]", me.name, me.email, me.role);
        }
        Commands::AuthToken => {
            let auth = require_auth()?;
            println!("{}", auth.token);
        }
        Commands::SessionsList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let sessions = client.list_sessions().await?;
            for s in &sessions {
                println!("{}  {}  {:?}", s.id, s.title, s.created_at);
            }
        }
        Commands::SessionsCreate { title } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let session = client.create_session(title.as_deref().unwrap_or("New session"), "research").await?;
            println!("Created session: {}", session.id);
        }
        Commands::SessionsGet { session_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let session = client.get_session(&session_id).await?;
            println!("{}  {}  {:?}", session.id, session.title, session.created_at);
        }
        Commands::SessionsDelete { session_id } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.delete_session(&session_id).await?;
            println!("Deleted session {session_id}");
        }
        Commands::Send { session_id, message } => {
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
            for m in &msgs {
                let text: String = m.content.iter()
                    .filter_map(|p| p.text.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("[{}] {}: {}", m.role, m.id, text.chars().take(200).collect::<String>());
            }
        }
        Commands::KnowledgeSearch { query } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let results = client.knowledge_search(&query, 5).await?;
            for r in &results {
                println!("{} [score={:.3}]: {}", r.id, r.score, r.text.chars().take(200).collect::<String>());
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
            for d in &docs {
                println!("{}", serde_json::to_string_pretty(d)?);
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
            for m in &meetings {
                println!("{}  {}  {:?}", m.id, m.title, m.date);
            }
        }
        Commands::Usage => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let usage = client.usage_summary().await?;
            for u in &usage {
                println!("{}: {} in / {} out / ${:.2}", u.email, u.total_input_tokens, u.total_output_tokens, u.total_cost_cents as f64 / 100.0);
            }
        }
        Commands::ApikeysList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let keys = client.list_api_keys(&auth.user_id).await?;
            for k in &keys {
                println!("{}  {}  {}", k.id, k.provider, k.user_id);
            }
        }
        Commands::ApikeysSet { provider, key } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.set_api_key(&provider, &key).await?;
            println!("Set API key for {provider}");
        }
        Commands::ApikeysDelete { provider } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.delete_api_key(&provider).await?;
            println!("Deleted API key for {provider}");
        }
        Commands::AuthKeyCreate { name } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let key = client.create_auth_key(&name).await?;
            println!("{}", serde_json::to_string_pretty(&key)?);
        }
        Commands::AuthKeyList => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            let keys = client.list_auth_keys().await?;
            for k in &keys {
                println!("{}  {}  {:?}", k.key_prefix, k.name, k.created_at);
            }
        }
        Commands::AuthKeyDelete { key_prefix } => {
            let auth = require_auth()?;
            let client = make_client(&auth);
            client.delete_auth_key(&key_prefix).await?;
            println!("Deleted key {key_prefix}");
        }
        Commands::Mcp => {
            let auth = require_auth()?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "mcpServers": {
                    "Kioku": {
                        "url": format!("{}/mcp", auth.server_url.trim_end_matches('/')),
                        "headers": {
                            "Authorization": format!("Bearer {}", auth.token)
                        }
                    }
                }
            }))?);
        }
        Commands::UpgradeCheck => {
            let info = cc_upgrade::check_for_update(REPO, VERSION).await?;
            if info.latest_version == VERSION {
                println!("Up to date (v{VERSION}).");
            } else {
                println!("New version available: v{} (current: v{VERSION})", info.latest_version);
            }
        }
        Commands::Upgrade => {
            let msg = cc_upgrade::perform_upgrade(REPO, VERSION).await?;
            println!("{msg}");
        }
    }
    Ok(())
}