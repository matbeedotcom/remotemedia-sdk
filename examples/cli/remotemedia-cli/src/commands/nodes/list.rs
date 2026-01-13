//! `remotemedia nodes list` - List available node types

use anyhow::Result;
use clap::Args;
use remotemedia_runtime_core::transport::PipelineExecutor;

use crate::output::{OutputFormat, Outputter};

#[derive(Args)]
pub struct ListArgs {
    /// Filter by name pattern
    #[arg(long)]
    pub filter: Option<String>,

    /// List from remote server
    #[arg(long)]
    pub server: Option<String>,
}

/// Node type information
#[derive(serde::Serialize)]
struct NodeInfo {
    node_type: String,
}

pub async fn execute(args: ListArgs, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // Get actual node types from runtime-core registry
    let executor = PipelineExecutor::new()
        .map_err(|e| anyhow::anyhow!("Failed to create executor: {}", e))?;

    let mut node_types = executor.list_node_types().await;

    // Apply filter
    if let Some(filter) = &args.filter {
        let filter_lower = filter.to_lowercase();
        node_types.retain(|n| n.to_lowercase().contains(&filter_lower));
    }

    let nodes: Vec<NodeInfo> = node_types
        .into_iter()
        .map(|node_type| NodeInfo { node_type })
        .collect();

    // Output based on format
    match format {
        OutputFormat::Json => {
            outputter.output(&nodes)?;
        }
        OutputFormat::Table => {
            use comfy_table::{presets::UTF8_FULL, Table};
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["NODE TYPE"]);

            for node in &nodes {
                table.add_row(vec![node.node_type.as_str()]);
            }

            println!("{table}");
        }
        OutputFormat::Text => {
            println!("{:<40}", "NODE TYPE");
            println!("{:-<40}", "");
            for node in &nodes {
                println!("{:<40}", node.node_type);
            }
        }
    }

    Ok(())
}
