//! `remotemedia servers add` - Add a saved server

use anyhow::Result;
use clap::Args;

use crate::config::{Config, ServerEntry, load_servers, save_servers};

#[derive(Args)]
pub struct AddArgs {
    /// Server name
    pub name: String,

    /// Server URL (grpc://, http://, ws://)
    pub url: String,

    /// Authentication token
    #[arg(long)]
    pub auth_token: Option<String>,

    /// Set as default server
    #[arg(long)]
    pub default: bool,
}

pub async fn execute(args: AddArgs, _config: &Config) -> Result<()> {
    // Validate URL
    if !args.url.starts_with("grpc://")
        && !args.url.starts_with("http://")
        && !args.url.starts_with("https://")
        && !args.url.starts_with("ws://")
        && !args.url.starts_with("wss://")
    {
        anyhow::bail!(
            "Invalid URL scheme. Must be grpc://, http://, https://, ws://, or wss://"
        );
    }

    let mut servers = load_servers()?;

    // Check if name already exists
    if servers.iter().any(|s| s.name == args.name) {
        anyhow::bail!("Server '{}' already exists. Remove it first.", args.name);
    }

    // If setting as default, clear other defaults
    if args.default {
        for server in &mut servers {
            server.is_default = false;
        }
    }

    // Add new server
    servers.push(ServerEntry {
        name: args.name.clone(),
        url: args.url.clone(),
        auth_token: args.auth_token,
        is_default: args.default,
    });

    save_servers(&servers)?;

    println!("Added server '{}' -> {}", args.name, args.url);
    if args.default {
        println!("Set as default server");
    }

    Ok(())
}
