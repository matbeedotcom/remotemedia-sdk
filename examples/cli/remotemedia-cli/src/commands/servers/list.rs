//! `remotemedia servers list` - List saved servers

use anyhow::Result;

use crate::config::{Config, load_servers};
use crate::output::OutputFormat;

pub async fn execute(_config: &Config, format: OutputFormat) -> Result<()> {
    let servers = load_servers()?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&servers)?);
        }
        OutputFormat::Table | OutputFormat::Text => {
            if servers.is_empty() {
                println!("No saved servers. Use 'remotemedia servers add' to add one.");
            } else {
                println!("{:<15} {:<40} {}", "NAME", "URL", "DEFAULT");
                println!("{:-<15} {:-<40} {:-<7}", "", "", "");
                for server in &servers {
                    println!(
                        "{:<15} {:<40} {}",
                        server.name,
                        server.url,
                        if server.is_default { "*" } else { "" }
                    );
                }
            }
        }
    }

    Ok(())
}
