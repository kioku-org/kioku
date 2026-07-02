use anyhow::Result;
use crate::cli::{Cli, Commands};
use crate::context::AppContext;
use crate::session;

pub mod auth;
pub mod calendar;
pub mod completions;
pub mod docs;
pub mod keys;
pub mod mcp;
pub mod meetings;
pub mod search;
pub mod upgrade;

pub async fn run(cli: Cli) -> Result<()> {
    if cli.token {
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

        Some(Commands::Signin { api_key, calendar }) => {
            auth::signin(ctx, api_key, calendar).await
        }

        Some(Commands::Signout) => auth::signout(),

        Some(Commands::Whoami) => auth::whoami().await,

        Some(Commands::Token | Commands::AuthToken) => auth::token(),

        Some(Commands::Search { query, limit }) => {
            search::run(ctx, query, limit).await
        }

        Some(Commands::Docs { path, delete }) => {
            docs::run(ctx, path, delete).await
        }

        Some(Commands::Meet { link, kill }) => {
            meetings::run(ctx, link, kill).await
        }

        Some(Commands::Transcript { meeting_id }) => {
            meetings::transcript(ctx, meeting_id).await
        }

        Some(Commands::Cal { week, date }) => {
            calendar::run(ctx, week, date).await
        }

        Some(Commands::Keys {
            create,
            name,
            delete,
        }) => {
            keys::run(ctx, create, name, delete).await
        }

        Some(Commands::Mcp) => mcp::run().await,

        Some(Commands::Upgrade) => upgrade::run().await,

        Some(Commands::Completions { shell }) => {
            completions::run(shell)
        }

        Some(Commands::RegisterAdmin {
            company_name,
            company_slug,
            email,
            name,
            password,
        }) => {
            auth::register_admin(
                ctx,
                company_name,
                company_slug,
                email,
                name,
                password,
            )
            .await
        }
    }
}