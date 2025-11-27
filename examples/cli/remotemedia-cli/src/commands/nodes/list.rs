//! `remotemedia nodes list` - List available node types

use anyhow::Result;
use clap::Args;

use crate::output::{OutputFormat, Outputter};

#[derive(Args)]
pub struct ListArgs {
    /// Filter by name pattern
    #[arg(long)]
    pub filter: Option<String>,

    /// Filter by category
    #[arg(long)]
    pub category: Option<String>,

    /// List from remote server
    #[arg(long)]
    pub server: Option<String>,
}

/// Node type information
#[derive(serde::Serialize)]
struct NodeInfo {
    node_type: String,
    category: String,
    streaming: bool,
    input_type: String,
    output_type: String,
}

pub async fn execute(args: ListArgs, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // TODO: Get actual node types from runtime-core registry
    // For now, provide a static list of known nodes
    let mut nodes = vec![
        NodeInfo {
            node_type: "SileroVADNode".to_string(),
            category: "audio".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Audio".to_string(),
        },
        NodeInfo {
            node_type: "WhisperNode".to_string(),
            category: "asr".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Text".to_string(),
        },
        NodeInfo {
            node_type: "KokoroTTSNode".to_string(),
            category: "tts".to_string(),
            streaming: true,
            input_type: "Text".to_string(),
            output_type: "Audio".to_string(),
        },
        NodeInfo {
            node_type: "RemotePipelineNode".to_string(),
            category: "routing".to_string(),
            streaming: true,
            input_type: "Any".to_string(),
            output_type: "Any".to_string(),
        },
        NodeInfo {
            node_type: "EchoNode".to_string(),
            category: "utility".to_string(),
            streaming: false,
            input_type: "Any".to_string(),
            output_type: "Any".to_string(),
        },
        NodeInfo {
            node_type: "PassthroughNode".to_string(),
            category: "utility".to_string(),
            streaming: true,
            input_type: "Any".to_string(),
            output_type: "Any".to_string(),
        },
        NodeInfo {
            node_type: "ResampleNode".to_string(),
            category: "audio".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Audio".to_string(),
        },
        NodeInfo {
            node_type: "FormatConverterNode".to_string(),
            category: "audio".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Audio".to_string(),
        },
    ];

    // Apply filters
    if let Some(filter) = &args.filter {
        let filter_lower = filter.to_lowercase();
        nodes.retain(|n| n.node_type.to_lowercase().contains(&filter_lower));
    }

    if let Some(category) = &args.category {
        let category_lower = category.to_lowercase();
        nodes.retain(|n| n.category.to_lowercase() == category_lower);
    }

    // Output based on format
    match format {
        OutputFormat::Json => {
            outputter.output(&nodes)?;
        }
        OutputFormat::Table => {
            use comfy_table::{presets::UTF8_FULL, Table};
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["NODE TYPE", "CATEGORY", "STREAMING", "INPUT", "OUTPUT"]);

            for node in &nodes {
                table.add_row(vec![
                    node.node_type.as_str(),
                    node.category.as_str(),
                    if node.streaming { "yes" } else { "no" },
                    node.input_type.as_str(),
                    node.output_type.as_str(),
                ]);
            }

            println!("{table}");
        }
        OutputFormat::Text => {
            println!("{:<25} {:<12} {:<10}", "NODE TYPE", "CATEGORY", "STREAMING");
            println!("{:-<25} {:-<12} {:-<10}", "", "", "");
            for node in &nodes {
                println!(
                    "{:<25} {:<12} {:<10}",
                    node.node_type,
                    node.category,
                    if node.streaming { "yes" } else { "no" }
                );
            }
        }
    }

    Ok(())
}
