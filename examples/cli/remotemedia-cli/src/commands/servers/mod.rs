//! Server management subcommands

pub mod add;
pub mod list;
pub mod remove;

use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;
use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum ServersCommand {
    /// List saved servers
    List,
    /// Add a saved server
    Add(add::AddArgs),
    /// Remove a saved server
    Remove(remove::RemoveArgs),
}

pub async fn execute(command: ServersCommand, config: &Config, format: OutputFormat) -> Result<()> {
    match command {
        ServersCommand::List => list::execute(config, format).await,
        ServersCommand::Add(args) => add::execute(args, config).await,
        ServersCommand::Remove(args) => remove::execute(args, config).await,
    }
}
