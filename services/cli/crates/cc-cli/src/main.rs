use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod context;
mod google_calendar;
mod output;
mod session;
mod signin;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(context::log_filter(cli.verbose))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    commands::run(cli).await
}