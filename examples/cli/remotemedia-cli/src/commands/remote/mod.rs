//! Remote execution subcommands

pub mod run;
pub mod stream;

use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;
use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum RemoteCommand {
    /// Execute pipeline on remote server
    Run(run::RemoteRunArgs),
    /// Stream to/from remote server
    Stream(stream::RemoteStreamArgs),
}

pub async fn execute(command: RemoteCommand, config: &Config, format: OutputFormat) -> Result<()> {
    match command {
        RemoteCommand::Run(args) => run::execute(args, config, format).await,
        RemoteCommand::Stream(args) => stream::execute(args, config, format).await,
    }
}
