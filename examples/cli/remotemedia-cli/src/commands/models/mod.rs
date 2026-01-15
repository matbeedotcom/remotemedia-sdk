//! Model management commands for Candle ML nodes

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub command: ModelsCommand,
}

#[derive(Subcommand)]
pub enum ModelsCommand {
    /// List cached models
    List,
    /// Show cache statistics
    Stats,
    /// Download a model for offline use
    Download {
        /// Model ID (e.g., "openai/whisper-base")
        model_id: String,
        /// Specific file to download
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Remove a model from cache
    Remove {
        /// Model ID to remove
        model_id: String,
    },
}

#[cfg(feature = "candle")]
pub async fn run(args: ModelsArgs) -> anyhow::Result<()> {
    use remotemedia_candle_nodes::ModelCli;
    
    let cli = ModelCli::new();
    
    match args.command {
        ModelsCommand::List => {
            cli.print_list()?;
        }
        ModelsCommand::Stats => {
            cli.print_stats()?;
        }
        ModelsCommand::Download { model_id, file } => {
            let filename = file.as_deref().unwrap_or("model.safetensors");
            println!("Downloading {} from {}...", filename, model_id);
            let path = cli.download(&model_id, filename).await?;
            println!("Downloaded to: {}", path.display());
        }
        ModelsCommand::Remove { model_id } => {
            if cli.remove(&model_id)? {
                println!("Removed: {}", model_id);
            } else {
                println!("Model not found in cache: {}", model_id);
            }
        }
    }
    
    Ok(())
}

#[cfg(not(feature = "candle"))]
pub async fn run(_args: ModelsArgs) -> anyhow::Result<()> {
    anyhow::bail!("Candle feature not enabled. Rebuild with --features candle")
}
