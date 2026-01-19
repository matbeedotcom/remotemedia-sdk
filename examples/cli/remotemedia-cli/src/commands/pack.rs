//! Pack command - Create self-contained Python wheels from pipelines

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

/// Arguments for the pack command
#[derive(Args, Debug)]
pub struct PackArgs {
    /// Path to the pipeline YAML manifest
    #[arg(required = true)]
    pipeline: PathBuf,

    /// Output directory for the generated package
    #[arg(short, long, default_value = "./dist")]
    output: PathBuf,

    /// Override the package name (default: from manifest)
    #[arg(short, long)]
    name: Option<String>,

    /// Package version
    #[arg(long, default_value = "0.1.0")]
    version: String,

    /// Build the wheel after generating
    #[arg(long)]
    build: bool,

    /// Build in release mode (requires --build)
    #[arg(long)]
    release: bool,

    /// Run tests after building (requires --build)
    #[arg(long)]
    test: bool,
}

/// Execute the pack command
pub async fn execute(args: PackArgs) -> Result<()> {
    // Find the workspace root by looking for Cargo.toml with [workspace]
    let workspace_root = find_workspace_root()?;
    
    // Build the pack tool first if needed
    println!("Building remotemedia-pack tool...");
    let status = Command::new("cargo")
        .args(["build", "-p", "remotemedia-pack", "--release"])
        .current_dir(&workspace_root)
        .status()
        .context("Failed to build remotemedia-pack")?;
    
    if !status.success() {
        anyhow::bail!("Failed to build remotemedia-pack");
    }
    
    // Find the built binary
    let pack_binary = workspace_root
        .join("target")
        .join("release")
        .join("remotemedia-pack");
    
    if !pack_binary.exists() {
        anyhow::bail!("remotemedia-pack binary not found at {:?}", pack_binary);
    }
    
    // Build the command
    let mut cmd = Command::new(&pack_binary);
    cmd.arg("python");
    cmd.arg(&args.pipeline);
    cmd.arg("--output").arg(&args.output);
    
    if let Some(name) = &args.name {
        cmd.arg("--name").arg(name);
    }
    
    cmd.arg("--version").arg(&args.version);
    
    if args.build {
        cmd.arg("--build");
    }
    
    if args.release {
        cmd.arg("--release");
    }
    
    if args.test {
        cmd.arg("--test");
    }
    
    // Add verbose flag
    cmd.arg("-v");
    
    println!("Packing pipeline: {:?}", args.pipeline);
    
    let status = cmd
        .status()
        .context("Failed to run remotemedia-pack")?;
    
    if !status.success() {
        anyhow::bail!("remotemedia-pack failed");
    }
    
    Ok(())
}

/// Find the workspace root directory
fn find_workspace_root() -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .context("Failed to locate workspace")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to locate workspace root");
    }
    
    let path = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in workspace path")?;
    
    let cargo_toml = PathBuf::from(path.trim());
    cargo_toml
        .parent()
        .map(|p| p.to_path_buf())
        .context("Failed to get workspace root directory")
}
