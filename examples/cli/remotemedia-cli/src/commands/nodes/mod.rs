//! Node management subcommands

pub mod info;
pub mod list;

use anyhow::Result;
use clap::Subcommand;

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum NodesCommand {
    /// List available node types
    List(list::ListArgs),
    /// Show details about a node type
    Info(info::InfoArgs),
}

pub async fn execute(command: NodesCommand, format: OutputFormat) -> Result<()> {
    match command {
        NodesCommand::List(args) => list::execute(args, format).await,
        NodesCommand::Info(args) => info::execute(args, format).await,
    }
}
