//! RemoteMedia Pack - Pipeline Packaging Tool
//!
//! Generates distributable Python or Node.js packages from pipeline YAML files.
//!
//! # Usage
//!
//! ```bash
//! # Generate a Python package from a pipeline YAML
//! remotemedia-pack python ./my-pipeline.yaml --output ./dist
//!
//! # Generate and build wheel in one step
//! remotemedia-pack python ./my-pipeline.yaml --output ./dist --build
//!
//! # Override package name
//! remotemedia-pack python ./my-pipeline.yaml --name my_custom_name
//! ```

mod generator;
mod node_resolver;
mod templates;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// RemoteMedia Pack - Package pipelines as distributable libraries
#[derive(Parser)]
#[command(name = "remotemedia-pack")]
#[command(author, version)]
#[command(about = "Generate Python/Node.js packages from RemoteMedia pipeline YAML files")]
struct Args {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a Python package (wheel) from a pipeline YAML
    Python {
        /// Path to the pipeline YAML file
        pipeline: PathBuf,

        /// Override package name (default: from pipeline metadata.name)
        #[arg(short, long)]
        name: Option<String>,

        /// Package version
        #[arg(short = 'V', long, default_value = "0.1.0")]
        version: String,

        /// Output directory for generated package
        #[arg(short, long, default_value = "./dist")]
        output: PathBuf,

        /// Build wheel after generating package
        #[arg(long)]
        build: bool,

        /// Build in release mode (implies --build)
        #[arg(long)]
        release: bool,

        /// Test wheel after building (implies --build)
        #[arg(long)]
        test: bool,

        /// Python version requirement (e.g., ">=3.10")
        #[arg(long, default_value = ">=3.10")]
        python_requires: String,

        /// Additional pip dependencies (can be specified multiple times)
        #[arg(long)]
        dependency: Vec<String>,

        /// Path to remotemedia-sdk workspace root (auto-detected if not specified)
        #[arg(long)]
        workspace_root: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    let filter = match args.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    match args.command {
        Command::Python {
            pipeline,
            name,
            version,
            output,
            build,
            release,
            test,
            python_requires,
            dependency,
            workspace_root,
        } => {
            // Auto-detect workspace root if not specified
            // The pack tool is at {workspace}/target/debug/remotemedia-pack
            let workspace_root = workspace_root.unwrap_or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| {
                        exe.parent()  // target/debug
                            .and_then(|p| p.parent())  // target
                            .and_then(|p| p.parent())  // workspace root
                            .map(|p| p.to_path_buf())
                    })
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            });

            tracing::debug!("Using workspace root: {:?}", workspace_root);

            let config = generator::PythonPackageConfig {
                pipeline_path: pipeline,
                name_override: name,
                version,
                output_dir: output,
                workspace_root,
                build_wheel: build || release || test,
                release_mode: release,
                test_wheel: test,
                python_requires,
                extra_dependencies: dependency,
            };

            generator::generate_python_package(config)
                .context("Failed to generate Python package")?;
        }
    }

    Ok(())
}
