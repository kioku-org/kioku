use crate::cli::{Cli, Commands};
use crate::context::AppContext;
use crate::session;
use anyhow::Result;

pub mod auth;
pub mod calendar;
pub mod completions;
pub mod docs;
pub mod invite;
pub mod keys;
pub mod mcp;
pub mod meetings;
pub mod search;
pub mod upgrade;
pub mod ws;

pub async fn run(cli: Cli) -> Result<()> {
    if cli.token {
        anyhow::ensure!(
            cli.command.is_none(),
            "--token can't be combined with a subcommand — run `kioku --token` on its own"
        );
        let auth = session::require_auth()?;
        println!("{}", auth.token);
        return Ok(());
    }

    let ctx = AppContext::new(cli.server, cli.json);

    match cli.command {
        None => {
            println!("kioku — context infrastructure client");
            println!("Run `kioku help` for available commands.");
            Ok(())
        }

        Some(Commands::Signin { api_key }) => auth::signin(ctx, api_key).await,

        Some(Commands::Signout) => auth::signout(),

        Some(Commands::Whoami) => auth::whoami(ctx).await,

        Some(Commands::AuthToken) => auth::token(),

        Some(Commands::Search { query, limit }) => search::run(ctx, query, limit).await,

        Some(Commands::Docs { path, delete }) => docs::run(ctx, path, delete).await,

        Some(Commands::Meet {
            link,
            kill,
            transcript,
            follow,
        }) => {
            if let Some(meeting_id) = transcript {
                if follow {
                    meetings::follow(ctx, meeting_id).await
                } else {
                    meetings::transcript(ctx, meeting_id).await
                }
            } else {
                meetings::run(ctx, link, kill).await
            }
        }

        Some(Commands::Cal { week, date }) => calendar::run(ctx, week, date).await,

        Some(Commands::Keys {
            create,
            name,
            delete,
        }) => keys::run(ctx, create, name, delete).await,

        Some(Commands::Ws {
            name,
            create,
            invite,
        }) => ws::run(ctx, name, create, invite).await,

        Some(Commands::Invite { email, revoke }) => invite::run(ctx, email, revoke).await,

        Some(Commands::Mcp) => mcp::run().await,

        Some(Commands::Upgrade) => upgrade::run().await,

        Some(Commands::Completions { shell }) => completions::run(shell),

        Some(Commands::RegisterAdmin {
            workspace_name,
            workspace_slug,
            email,
            name,
            password,
        }) => {
            auth::register_admin(ctx, workspace_name, workspace_slug, email, name, password).await
        }
    }
}
