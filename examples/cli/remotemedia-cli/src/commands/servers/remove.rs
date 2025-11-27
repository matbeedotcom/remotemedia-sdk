//! `remotemedia servers remove` - Remove a saved server

use anyhow::Result;
use clap::Args;

use crate::config::{Config, load_servers, save_servers};

#[derive(Args)]
pub struct RemoveArgs {
    /// Server name to remove
    pub name: String,
}

pub async fn execute(args: RemoveArgs, _config: &Config) -> Result<()> {
    let mut servers = load_servers()?;

    let original_len = servers.len();
    servers.retain(|s| s.name != args.name);

    if servers.len() == original_len {
        eprintln!("Server '{}' not found", args.name);
        std::process::exit(1);
    }

    save_servers(&servers)?;

    println!("Removed server '{}'", args.name);

    Ok(())
}
