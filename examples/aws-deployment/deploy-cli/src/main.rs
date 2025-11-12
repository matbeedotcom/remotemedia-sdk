#!/usr/bin/env cargo
//! RemoteMedia AWS Deployment CLI
//!
//! This tool helps deploy RemotePipelineNode servers to AWS and generate
//! local configuration for connecting to them.
//!
//! # Usage
//!
//! ```bash
//! # Build Docker image and push to ECR
//! cargo run -- build --push
//!
//! # Deploy via Terraform
//! cargo run -- deploy --terraform
//!
//! # Deploy via CDK
//! cargo run -- deploy --cdk
//!
//! # Generate local manifest with remote nodes
//! cargo run -- generate-manifest --endpoint lb-123.elb.amazonaws.com:50051
//!
//! # List deployed services
//! cargo run -- list
//!
//! # Teardown deployment
//! cargo run -- destroy
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::process::Command;

#[derive(Parser)]
#[command(name = "remotemedia-deploy")]
#[command(about = "Deploy RemoteMedia pipeline nodes to AWS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// AWS region
    #[arg(long, default_value = "us-east-1", global = true)]
    region: String,

    /// AWS profile
    #[arg(long, global = true)]
    profile: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Build Docker image and optionally push to ECR
    Build {
        /// Push to ECR after building
        #[arg(long)]
        push: bool,

        /// ECR repository name
        #[arg(long, default_value = "remotemedia-grpc")]
        repo: String,
    },

    /// Deploy infrastructure
    Deploy {
        /// Use Terraform
        #[arg(long, group = "method")]
        terraform: bool,

        /// Use AWS CDK
        #[arg(long, group = "method")]
        cdk: bool,

        /// Working directory for deployment files
        #[arg(long, default_value = "../terraform")]
        workdir: String,
    },

    /// Destroy deployed infrastructure
    Destroy {
        /// Use Terraform
        #[arg(long, group = "method")]
        terraform: bool,

        /// Use AWS CDK
        #[arg(long, group = "method")]
        cdk: bool,

        /// Auto-approve destruction (skip confirmation)
        #[arg(long)]
        yes: bool,
    },

    /// Generate local pipeline manifest with remote nodes
    GenerateManifest {
        /// Remote gRPC endpoint (e.g., lb-123.elb.amazonaws.com:50051)
        #[arg(long)]
        endpoint: String,

        /// Output file path
        #[arg(short, long, default_value = "pipeline-with-remote.json")]
        output: String,

        /// Pipeline type to generate
        #[arg(long, default_value = "stt-tts")]
        template: String,
    },

    /// List deployed services and their endpoints
    List,

    /// Upload pipeline manifest to S3
    UploadManifest {
        /// Local manifest file path
        #[arg(long)]
        file: String,

        /// S3 bucket name (auto-detected if not specified)
        #[arg(long)]
        bucket: Option<String>,

        /// S3 key (filename in bucket)
        #[arg(long)]
        key: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { push, repo } => build_image(&cli, push, &repo),
        Commands::Deploy { terraform, cdk, workdir } => {
            if terraform {
                deploy_terraform(&cli, &workdir)
            } else if cdk {
                deploy_cdk(&cli)
            } else {
                println!("Please specify --terraform or --cdk");
                Ok(())
            }
        }
        Commands::Destroy { terraform, cdk, yes } => {
            if terraform {
                destroy_terraform(&cli, yes)
            } else if cdk {
                destroy_cdk(&cli, yes)
            } else {
                println!("Please specify --terraform or --cdk");
                Ok(())
            }
        }
        Commands::GenerateManifest { endpoint, output, template } => {
            generate_manifest(&endpoint, &output, &template)
        }
        Commands::List => list_services(&cli),
        Commands::UploadManifest { file, bucket, key } => {
            upload_manifest(&cli, &file, bucket.as_deref(), key.as_deref())
        }
    }
}

fn build_image(cli: &Cli, push: bool, repo: &str) -> Result<()> {
    println!("🔨 Building Docker image for RemoteMedia gRPC server...");

    // Build with cross-compilation support
    let status = Command::new("docker")
        .args(&[
            "build",
            "-t",
            &format!("{}:latest", repo),
            "-f",
            "../../../Dockerfile",
            "../../../",
        ])
        .status()
        .context("Failed to run docker build")?;

    if !status.success() {
        anyhow::bail!("Docker build failed");
    }

    if push {
        println!("📤 Pushing to ECR...");

        // Get AWS account ID
        let account_id = get_aws_account_id(cli)?;

        // ECR login
        println!("🔐 Logging into ECR...");
        let ecr_endpoint = format!("{}.dkr.ecr.{}.amazonaws.com", account_id, cli.region);

        let login_cmd = if let Some(profile) = &cli.profile {
            format!("aws ecr get-login-password --region {} --profile {} | docker login --username AWS --password-stdin {}", 
                cli.region, profile, ecr_endpoint)
        } else {
            format!("aws ecr get-login-password --region {} | docker login --username AWS --password-stdin {}", 
                cli.region, ecr_endpoint)
        };

        let status = Command::new("sh")
            .arg("-c")
            .arg(&login_cmd)
            .status()
            .context("Failed to login to ECR")?;

        if !status.success() {
            anyhow::bail!("ECR login failed");
        }

        // Tag and push
        let ecr_image = format!("{}/{}:latest", ecr_endpoint, repo);
        Command::new("docker")
            .args(&["tag", &format!("{}:latest", repo), &ecr_image])
            .status()
            .context("Failed to tag image")?;

        let status = Command::new("docker")
            .args(&["push", &ecr_image])
            .status()
            .context("Failed to push to ECR")?;

        if !status.success() {
            anyhow::bail!("Docker push failed");
        }

        println!("✅ Image pushed to {}", ecr_image);
    }

    Ok(())
}

fn deploy_terraform(cli: &Cli, workdir: &str) -> Result<()> {
    println!("🚀 Deploying infrastructure with Terraform...");

    // Initialize Terraform
    println!("Initializing Terraform...");
    let mut cmd = Command::new("terraform");
    cmd.arg("init").current_dir(workdir);
    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }
    let status = cmd.status().context("Failed to run terraform init")?;
    if !status.success() {
        anyhow::bail!("Terraform init failed");
    }

    // Plan
    println!("Planning deployment...");
    let mut cmd = Command::new("terraform");
    cmd.args(&["plan", "-out=tfplan"])
        .current_dir(workdir)
        .arg(format!("-var=aws_region={}", cli.region));
    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }
    let status = cmd.status().context("Failed to run terraform plan")?;
    if !status.success() {
        anyhow::bail!("Terraform plan failed");
    }

    // Apply
    println!("Applying deployment...");
    let mut cmd = Command::new("terraform");
    cmd.args(&["apply", "tfplan"]).current_dir(workdir);
    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }
    let status = cmd.status().context("Failed to run terraform apply")?;
    if !status.success() {
        anyhow::bail!("Terraform apply failed");
    }

    // Get outputs
    println!("\n📊 Deployment outputs:");
    let mut cmd = Command::new("terraform");
    cmd.args(&["output", "-json"]).current_dir(workdir);
    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }
    let output = cmd.output().context("Failed to get terraform outputs")?;
    
    if output.status.success() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    println!("\n✅ Deployment complete!");

    Ok(())
}

fn deploy_cdk(cli: &Cli) -> Result<()> {
    println!("🚀 Deploying infrastructure with AWS CDK...");

    let mut cmd = Command::new("cdk");
    cmd.args(&["deploy", "--require-approval", "never"])
        .current_dir("../");

    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }

    let status = cmd.status().context("Failed to run cdk deploy")?;

    if !status.success() {
        anyhow::bail!("CDK deploy failed");
    }

    println!("✅ Deployment complete!");

    Ok(())
}

fn destroy_terraform(cli: &Cli, yes: bool) -> Result<()> {
    println!("🗑️  Destroying Terraform infrastructure...");

    let mut cmd = Command::new("terraform");
    cmd.arg("destroy").current_dir("../terraform");

    if yes {
        cmd.arg("-auto-approve");
    }

    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }

    let status = cmd.status().context("Failed to run terraform destroy")?;

    if !status.success() {
        anyhow::bail!("Terraform destroy failed");
    }

    println!("✅ Infrastructure destroyed");

    Ok(())
}

fn destroy_cdk(cli: &Cli, yes: bool) -> Result<()> {
    println!("🗑️  Destroying CDK infrastructure...");

    let mut cmd = Command::new("cdk");
    cmd.arg("destroy").current_dir("../");

    if yes {
        cmd.arg("--force");
    }

    if let Some(profile) = &cli.profile {
        cmd.env("AWS_PROFILE", profile);
    }

    let status = cmd.status().context("Failed to run cdk destroy")?;

    if !status.success() {
        anyhow::bail!("CDK destroy failed");
    }

    println!("✅ Infrastructure destroyed");

    Ok(())
}

fn generate_manifest(endpoint: &str, output: &str, template: &str) -> Result<()> {
    println!("📝 Generating local manifest with remote nodes...");

    let manifest = match template {
        "stt-tts" => json!({
            "version": "v1",
            "metadata": {
                "name": "local-with-remote-stt-tts"
            },
            "nodes": [
                {
                    "id": "vad",
                    "node_type": "SileroVAD",
                    "params": {
                        "threshold": 0.5
                    }
                },
                {
                    "id": "remote_stt",
                    "node_type": "RemotePipelineNode",
                    "params": {
                        "transport": "grpc",
                        "endpoint": endpoint,
                        "manifest": {
                            "version": "v1",
                            "nodes": [{
                                "id": "whisper",
                                "node_type": "WhisperSTT",
                                "params": {
                                    "model": "large-v3"
                                }
                            }]
                        },
                        "timeout_ms": 30000,
                        "retry": {
                            "max_retries": 3,
                            "backoff_ms": 1000
                        }
                    }
                },
                {
                    "id": "remote_tts",
                    "node_type": "RemotePipelineNode",
                    "params": {
                        "transport": "grpc",
                        "endpoint": endpoint,
                        "manifest": {
                            "version": "v1",
                            "nodes": [{
                                "id": "kokoro",
                                "node_type": "KokoroTTS",
                                "params": {
                                    "voice": "af_bella"
                                }
                            }]
                        },
                        "timeout_ms": 10000
                    }
                }
            ],
            "connections": [
                {"from": "vad", "to": "remote_stt"},
                {"from": "remote_stt", "to": "remote_tts"}
            ]
        }),
        _ => anyhow::bail!("Unknown template: {}", template),
    };

    std::fs::write(output, serde_json::to_string_pretty(&manifest)?)?;

    println!("✅ Manifest written to {}", output);
    println!("\n💡 Run locally with:");
    println!("   cargo run --bin grpc-server -- --manifest {}", output);

    Ok(())
}

fn list_services(cli: &Cli) -> Result<()> {
    println!("📋 Listing deployed RemoteMedia services...\n");

    // Query ECS services
    let mut cmd = Command::new("aws");
    cmd.args(&[
        "ecs",
        "list-services",
        "--cluster",
        "remotemedia-cluster",
        "--region",
        &cli.region,
    ]);

    if let Some(profile) = &cli.profile {
        cmd.arg("--profile").arg(profile);
    }

    let output = cmd.output().context("Failed to list ECS services")?;

    if !output.status.success() {
        eprintln!("Failed to list services: {}", String::from_utf8_lossy(&output.stderr));
        return Ok(());
    }

    println!("ECS Services:");
    println!("{}", String::from_utf8_lossy(&output.stdout));

    // Get load balancer endpoints
    let mut cmd = Command::new("aws");
    cmd.args(&[
        "elbv2",
        "describe-load-balancers",
        "--region",
        &cli.region,
    ]);

    if let Some(profile) = &cli.profile {
        cmd.arg("--profile").arg(profile);
    }

    let output = cmd.output().context("Failed to list load balancers")?;

    if output.status.success() {
        println!("\nLoad Balancers:");
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

fn upload_manifest(cli: &Cli, file: &str, bucket: Option<&str>, key: Option<&str>) -> Result<()> {
    // Auto-detect bucket if not provided
    let bucket_name = if let Some(b) = bucket {
        b.to_string()
    } else {
        let account_id = get_aws_account_id(cli)?;
        format!("remotemedia-manifests-{}", account_id)
    };

    let s3_key = key.unwrap_or_else(|| {
        std::path::Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("manifest.json")
    });

    println!("📤 Uploading {} to s3://{}/{}", file, bucket_name, s3_key);

    let mut cmd = Command::new("aws");
    cmd.args(&[
        "s3",
        "cp",
        file,
        &format!("s3://{}/{}", bucket_name, s3_key),
        "--region",
        &cli.region,
    ]);

    if let Some(profile) = &cli.profile {
        cmd.arg("--profile").arg(profile);
    }

    let status = cmd.status().context("Failed to upload to S3")?;

    if !status.success() {
        anyhow::bail!("S3 upload failed");
    }

    println!("✅ Manifest uploaded successfully!");
    println!("\n💡 Use in RemotePipelineNode:");
    println!(r#"  "manifest_url": "https://{}.s3.amazonaws.com/{}""#, bucket_name, s3_key);

    Ok(())
}

fn get_aws_account_id(cli: &Cli) -> Result<String> {
    let mut cmd = Command::new("aws");
    cmd.args(&["sts", "get-caller-identity", "--query", "Account", "--output", "text"]);

    if let Some(profile) = &cli.profile {
        cmd.arg("--profile").arg(profile);
    }

    let output = cmd.output().context("Failed to get AWS account ID")?;

    if !output.status.success() {
        anyhow::bail!("Failed to get AWS account ID");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

