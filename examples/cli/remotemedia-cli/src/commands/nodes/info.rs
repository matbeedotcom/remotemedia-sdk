//! `remotemedia nodes info` - Show details about a node type

use anyhow::Result;
use clap::Args;

use crate::output::{OutputFormat, Outputter};

#[derive(Args)]
pub struct InfoArgs {
    /// Node type name
    pub node_type: String,
}

#[derive(serde::Serialize)]
struct NodeDetails {
    node_type: String,
    category: String,
    streaming: bool,
    input_type: String,
    output_type: String,
    description: String,
    parameters: Vec<ParamInfo>,
}

#[derive(serde::Serialize)]
struct ParamInfo {
    name: String,
    param_type: String,
    default: String,
    description: String,
}

pub async fn execute(args: InfoArgs, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // TODO: Get actual node info from runtime-core registry
    // For now, provide static info for known nodes
    let details = match args.node_type.as_str() {
        "SileroVADNode" => NodeDetails {
            node_type: "SileroVADNode".to_string(),
            category: "audio".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Audio".to_string(),
            description: "Voice Activity Detection using Silero VAD model".to_string(),
            parameters: vec![
                ParamInfo {
                    name: "threshold".to_string(),
                    param_type: "float".to_string(),
                    default: "0.5".to_string(),
                    description: "Detection threshold (0.0-1.0)".to_string(),
                },
                ParamInfo {
                    name: "min_speech_duration_ms".to_string(),
                    param_type: "int".to_string(),
                    default: "250".to_string(),
                    description: "Minimum speech duration in milliseconds".to_string(),
                },
                ParamInfo {
                    name: "min_silence_duration_ms".to_string(),
                    param_type: "int".to_string(),
                    default: "100".to_string(),
                    description: "Minimum silence duration to end speech".to_string(),
                },
            ],
        },
        "WhisperNode" => NodeDetails {
            node_type: "WhisperNode".to_string(),
            category: "asr".to_string(),
            streaming: true,
            input_type: "Audio".to_string(),
            output_type: "Text".to_string(),
            description: "Speech-to-text transcription using Whisper".to_string(),
            parameters: vec![
                ParamInfo {
                    name: "model".to_string(),
                    param_type: "string".to_string(),
                    default: "base".to_string(),
                    description: "Model size: tiny, base, small, medium, large".to_string(),
                },
                ParamInfo {
                    name: "language".to_string(),
                    param_type: "string".to_string(),
                    default: "auto".to_string(),
                    description: "Target language (auto for detection)".to_string(),
                },
                ParamInfo {
                    name: "word_timestamps".to_string(),
                    param_type: "bool".to_string(),
                    default: "false".to_string(),
                    description: "Include word-level timestamps".to_string(),
                },
            ],
        },
        "KokoroTTSNode" => NodeDetails {
            node_type: "KokoroTTSNode".to_string(),
            category: "tts".to_string(),
            streaming: true,
            input_type: "Text".to_string(),
            output_type: "Audio".to_string(),
            description: "Text-to-speech synthesis using Kokoro".to_string(),
            parameters: vec![
                ParamInfo {
                    name: "voice".to_string(),
                    param_type: "string".to_string(),
                    default: "af_bella".to_string(),
                    description: "Voice selection".to_string(),
                },
                ParamInfo {
                    name: "speed".to_string(),
                    param_type: "float".to_string(),
                    default: "1.0".to_string(),
                    description: "Speech speed multiplier".to_string(),
                },
            ],
        },
        "RemotePipelineNode" => NodeDetails {
            node_type: "RemotePipelineNode".to_string(),
            category: "routing".to_string(),
            streaming: true,
            input_type: "Any".to_string(),
            output_type: "Any".to_string(),
            description: "Delegates processing to a remote server".to_string(),
            parameters: vec![
                ParamInfo {
                    name: "endpoint".to_string(),
                    param_type: "string".to_string(),
                    default: "".to_string(),
                    description: "Remote server URL (grpc://, http://, ws://)".to_string(),
                },
                ParamInfo {
                    name: "pipeline_name".to_string(),
                    param_type: "string".to_string(),
                    default: "".to_string(),
                    description: "Named pipeline on remote server".to_string(),
                },
                ParamInfo {
                    name: "timeout_ms".to_string(),
                    param_type: "int".to_string(),
                    default: "30000".to_string(),
                    description: "Request timeout in milliseconds".to_string(),
                },
            ],
        },
        _ => {
            eprintln!("Node type '{}' not found", args.node_type);
            std::process::exit(1);
        }
    };

    match format {
        OutputFormat::Json => {
            outputter.output(&details)?;
        }
        OutputFormat::Text | OutputFormat::Table => {
            println!("Node: {}", details.node_type);
            println!("Category: {}", details.category);
            println!("Streaming: {}", if details.streaming { "yes" } else { "no" });
            println!("Input: {}", details.input_type);
            println!("Output: {}", details.output_type);
            println!();
            println!("Description:");
            println!("  {}", details.description);
            println!();
            println!("Parameters:");
            for param in &details.parameters {
                println!(
                    "  {} ({}, default: \"{}\"): {}",
                    param.name, param.param_type, param.default, param.description
                );
            }
        }
    }

    Ok(())
}
