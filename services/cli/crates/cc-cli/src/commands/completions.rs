use crate::cli::Cli;
use anyhow::Result;
use clap::CommandFactory;

pub fn run(shell: clap_complete::Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
