use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kioku",
    version,
    about = "Kioku — context infrastructure client"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short = 'C', long, global = true)]
    pub cwd: Option<PathBuf>,

    #[arg(long, global = true)]
    pub server: Option<String>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    Signin {
        #[arg(long)]
        api_key: Option<String>,
    },

    Signout,
    Whoami,
    Token,

    Search {
        query: String,

        #[arg(long, default_value_t = 5)]
        limit: u32,
    },

    Docs {
        #[arg(value_name = "PATH")]
        path: Option<String>,

        #[arg(long, value_name = "DOCUMENT_ID", conflicts_with = "path")]
        delete: Option<String>,
    },

    Transcript {
        #[arg(value_name = "MEETING_ID")]
        meeting_id: String,
    },

    Meet {
        #[arg(value_name = "LINK")]
        link: Option<String>,

        #[arg(long, value_name = "BOT", conflicts_with = "link")]
        kill: Option<String>,
    },

    Cal {
        #[arg(long)]
        week: bool,

        #[arg(long, value_name = "DD/MM/YYYY", conflicts_with = "week")]
        date: Option<String>,
    },

    Keys {
        #[arg(long)]
        create: bool,

        #[arg(long, default_value = "cli-key")]
        name: String,

        #[arg(long, value_name = "KEY_PREFIX_OR_ID", conflicts_with = "create")]
        delete: Option<String>,
    },

    Mcp,
    Upgrade,

    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    }
}