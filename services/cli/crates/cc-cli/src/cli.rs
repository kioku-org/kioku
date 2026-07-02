use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "kioku",
    version,
    about = "Kioku — context infrastructure client"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long, global = true)]
    pub server: Option<String>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(
        long,
        global = true,
        help = "Print machine-readable JSON instead of formatted text"
    )]
    pub json: bool,

    #[arg(
        short = 't',
        long = "token",
        global = true,
        help = "Print stored auth token (useful for scripting)"
    )]
    pub token: bool,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    // ── Auth ──────────────────────────────────────────────────────────────────
    #[command(about = "Sign in with Google, GitHub or an API key")]
    Signin {
        #[arg(long, help = "Sign in using a long-lived API key instead of OAuth")]
        api_key: Option<String>,
    },
    #[command(about = "Sign out and clear stored credentials")]
    Signout,
    #[command(about = "Show current user and server")]
    Whoami,

    // ── Knowledge ─────────────────────────────────────────────────────────────
    #[command(about = "Search your knowledge base")]
    Search {
        query: String,
        #[arg(long, default_value_t = 5, help = "Max results to return")]
        limit: u32,
    },
    #[command(about = "List documents, upload a new one, or delete one")]
    Docs {
        #[arg(value_name = "PATH", help = "Path to a file to upload")]
        path: Option<String>,
        #[arg(
            long,
            value_name = "DOCUMENT_ID",
            conflicts_with = "path",
            help = "Delete a document by id"
        )]
        delete: Option<String>,
    },

    // ── Meetings ──────────────────────────────────────────────────────────────
    #[command(about = "List running meeting bots, join a meeting, or kill a bot")]
    Meet {
        #[arg(
            value_name = "LINK",
            help = "Meeting link to join (Google Meet, Teams, or Zoom — auto-detected). Pass `ls` to list."
        )]
        link: Option<String>,
        #[arg(
            long,
            value_name = "BOT",
            conflicts_with = "link",
            help = "Stop a running meeting bot, by meeting id or id prefix"
        )]
        kill: Option<String>,
        #[arg(
            long,
            value_name = "MEETING_ID",
            conflicts_with_all = ["link", "kill"],
            help = "Print the transcript for a specific meeting"
        )]
        transcript: Option<String>,
    },
    #[command(about = "List Google Calendar meetings for today, the coming week, or a date")]
    Cal {
        #[arg(long, help = "List the coming week instead of just today")]
        week: bool,
        #[arg(
            long,
            value_name = "DD/MM/YYYY",
            conflicts_with = "week",
            help = "List a specific date instead of today"
        )]
        date: Option<String>,
    },

    // ── API keys ──────────────────────────────────────────────────────────────
    #[command(about = "List API keys, create a new one, or delete one")]
    Keys {
        #[arg(long, help = "Create a new API key")]
        create: bool,
        #[arg(
            long,
            default_value = "cli-key",
            help = "Name for the new key (used with --create)"
        )]
        name: String,
        #[arg(
            long,
            value_name = "KEY_PREFIX_OR_ID",
            conflicts_with = "create",
            help = "Delete an API key by id or prefix"
        )]
        delete: Option<String>,
    },

    // ── Workspaces ────────────────────────────────────────────────────────────
    #[command(about = "List your workspaces, switch the active one, or create a new one")]
    Ws {
        #[arg(
            value_name = "NAME",
            help = "Workspace name or slug to switch to (or, with --invite, to invite into)"
        )]
        name: Option<String>,
        #[arg(
            long,
            value_name = "NAME",
            conflicts_with = "name",
            help = "Create a new workspace and switch to it"
        )]
        create: Option<String>,
        #[arg(
            long,
            value_name = "EMAIL",
            requires = "name",
            help = "Invite an email into the workspace given by NAME"
        )]
        invite: Option<String>,
    },

    // ── Teammates (Pro/Teams tier) ───────────────────────────────────────────
    #[command(
        about = "List pending invites, invite a teammate to your workspace, or revoke an invite"
    )]
    Invite {
        #[arg(value_name = "EMAIL", help = "Email address to invite")]
        email: Option<String>,
        #[arg(
            long,
            value_name = "INVITE_ID",
            conflicts_with = "email",
            help = "Revoke a pending invite by id"
        )]
        revoke: Option<String>,
    },

    // ── Tools ─────────────────────────────────────────────────────────────────
    #[command(about = "Print MCP server config for AI clients (Claude, Cursor, etc.)")]
    Mcp,
    #[command(about = "Check for updates and upgrade if a newer version is available")]
    Upgrade,
    #[command(about = "Generate shell completion script")]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    // ── Hidden / power-user ───────────────────────────────────────────────────
    #[command(hide = true)]
    RegisterAdmin {
        #[arg(long)]
        workspace_name: Option<String>,
        #[arg(long)]
        workspace_slug: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
    #[command(hide = true, name = "auth-token")]
    AuthToken,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn cli_parses_register_admin_with_server_override() {
        let fixture = [
            "kioku",
            "--server",
            "http://localhost:9100",
            "register-admin",
            "--workspace-name",
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
            workspace_name: Some("Kioku".to_string()),
            workspace_slug: None,
            email: Some("admin@example.com".to_string()),
            name: Some("Admin".to_string()),
            password: Some("password123".to_string()),
        });

        assert_eq!(actual.command, expected);
        assert_eq!(actual.server, Some("http://localhost:9100".to_string()));
    }
}
