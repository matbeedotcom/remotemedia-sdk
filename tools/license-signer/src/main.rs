//! License Signing Tool
//!
//! Internal tool for generating signed evaluation license files.
//! The private signing key must be kept secure and is never shipped in the SDK.
//!
//! # Usage
//!
//! ```bash
//! # Generate a new keypair
//! license-signer generate-keypair --output keys/
//!
//! # Sign a license
//! license-signer sign \
//!     --key-file keys/private.key \
//!     --customer "ACME Corp" \
//!     --customer-id "c9f4e3d2-b1a0-4f8e-9d6c-5b4a3e2f1d0c" \
//!     --expires "2026-07-01" \
//!     --watermark "EVAL-ACME-CORP" \
//!     --output license.json
//!
//! # Print public key as Rust const
//! license-signer print-public-key --key-file keys/private.key
//! ```

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{NaiveDate, Utc};
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json_canonicalizer::to_vec as canonicalize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

/// License signing tool for RemoteMedia SDK evaluation licenses
#[derive(Parser)]
#[command(name = "license-signer")]
#[command(author, version)]
#[command(about = "Generate and sign evaluation license files")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new Ed25519 keypair
    GenerateKeypair {
        /// Output directory for keypair files
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },

    /// Sign a license payload and generate a license file
    Sign {
        /// Path to the private key file
        #[arg(short, long)]
        key_file: PathBuf,

        /// Customer name (for display)
        #[arg(long)]
        customer: String,

        /// Customer ID (UUID, will be generated if not provided)
        #[arg(long)]
        customer_id: Option<String>,

        /// License ID (UUID, will be generated if not provided)
        #[arg(long)]
        license_id: Option<String>,

        /// Expiration date (YYYY-MM-DD)
        #[arg(long)]
        expires: String,

        /// Valid-from date (YYYY-MM-DD, optional)
        #[arg(long)]
        not_before: Option<String>,

        /// Watermark text to embed in outputs
        #[arg(long)]
        watermark: String,

        /// Allowed ingest schemes (comma-separated, default: file,udp,srt,rtmp)
        #[arg(long, default_value = "file,udp,srt,rtmp")]
        ingest_schemes: String,

        /// Allow video processing (default: true)
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        allow_video: bool,

        /// Max session duration in seconds (optional, unlimited if not set)
        #[arg(long)]
        max_session_secs: Option<u64>,

        /// Output license file path
        #[arg(short, long, default_value = "license.json")]
        output: PathBuf,

        /// Also output a compact binary license file (.lic)
        #[arg(long)]
        binary: bool,
    },

    /// Print the public key as a Rust const array
    PrintPublicKey {
        /// Path to the private key file
        #[arg(short, long)]
        key_file: PathBuf,
    },

    /// Verify a license file signature
    Verify {
        /// Path to the license file
        #[arg(short, long)]
        license_file: PathBuf,

        /// Path to the private key file (to derive public key)
        #[arg(short, long)]
        key_file: PathBuf,
    },
}

/// License payload structure (for canonicalization and signing)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicensePayload {
    version: u32,
    customer_id: String,
    license_id: String,
    issued_at: String,
    expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    not_before: Option<String>,
    entitlements: BTreeMap<String, serde_json::Value>,
    watermark: String,
}

/// Full license structure (with signature)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct License {
    version: u32,
    customer_id: String,
    license_id: String,
    issued_at: String,
    expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    not_before: Option<String>,
    entitlements: Entitlements,
    watermark: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entitlements {
    allow_ingest_schemes: Vec<String>,
    allow_video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_session_duration_secs: Option<u64>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::GenerateKeypair { output } => generate_keypair(&output),
        Command::Sign {
            key_file,
            customer,
            customer_id,
            license_id,
            expires,
            not_before,
            watermark,
            ingest_schemes,
            allow_video,
            max_session_secs,
            output,
            binary: _, // Handled by shell script, not Rust
        } => sign_license(
            &key_file,
            &customer,
            customer_id,
            license_id,
            &expires,
            not_before,
            &watermark,
            &ingest_schemes,
            allow_video,
            max_session_secs,
            &output,
        ),
        Command::PrintPublicKey { key_file } => print_public_key(&key_file),
        Command::Verify {
            license_file,
            key_file,
        } => verify_license(&license_file, &key_file),
    }
}

fn generate_keypair(output_dir: &PathBuf) -> Result<()> {
    // Create output directory if it doesn't exist
    fs::create_dir_all(output_dir).context("Failed to create output directory")?;

    // Generate a new keypair
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    // Save private key (base64 encoded)
    let private_key_path = output_dir.join("private.key");
    let private_key_bytes = signing_key.to_bytes();
    let private_key_b64 = BASE64.encode(private_key_bytes);
    fs::write(&private_key_path, &private_key_b64).context("Failed to write private key")?;

    // Save public key (base64 encoded)
    let public_key_path = output_dir.join("public.key");
    let public_key_bytes = verifying_key.to_bytes();
    let public_key_b64 = BASE64.encode(public_key_bytes);
    fs::write(&public_key_path, &public_key_b64).context("Failed to write public key")?;

    println!("Generated keypair:");
    println!("  Private key: {}", private_key_path.display());
    println!("  Public key:  {}", public_key_path.display());
    println!();
    println!("SECURITY: Keep the private key secure! Never commit it to version control.");
    println!();
    println!("To embed the public key in the demo binary, run:");
    println!("  license-signer print-public-key --key-file {}", private_key_path.display());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sign_license(
    key_file: &PathBuf,
    customer: &str,
    customer_id: Option<String>,
    license_id: Option<String>,
    expires: &str,
    not_before: Option<String>,
    watermark: &str,
    ingest_schemes: &str,
    allow_video: bool,
    max_session_secs: Option<u64>,
    output: &PathBuf,
) -> Result<()> {
    // Load private key
    let key_b64 = fs::read_to_string(key_file).context("Failed to read private key file")?;
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .context("Failed to decode private key")?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid private key length"))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);

    // Generate IDs if not provided
    let customer_id = customer_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let license_id = license_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // Parse dates
    let expires_date =
        NaiveDate::parse_from_str(expires, "%Y-%m-%d").context("Invalid expires date format")?;
    let expires_at = expires_date
        .and_hms_opt(23, 59, 59)
        .unwrap()
        .and_utc()
        .to_rfc3339();

    let not_before_at = if let Some(nb) = not_before {
        let nb_date =
            NaiveDate::parse_from_str(&nb, "%Y-%m-%d").context("Invalid not_before date format")?;
        Some(nb_date.and_hms_opt(0, 0, 0).unwrap().and_utc().to_rfc3339())
    } else {
        None
    };

    let issued_at = Utc::now().to_rfc3339();

    // Parse ingest schemes
    let schemes: Vec<String> = ingest_schemes
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();

    // Build entitlements as BTreeMap for consistent ordering
    let mut entitlements_map = BTreeMap::new();
    entitlements_map.insert(
        "allow_ingest_schemes".to_string(),
        serde_json::json!(schemes),
    );
    entitlements_map.insert("allow_video".to_string(), serde_json::json!(allow_video));
    if let Some(max_secs) = max_session_secs {
        entitlements_map.insert(
            "max_session_duration_secs".to_string(),
            serde_json::json!(max_secs),
        );
    }

    // Create payload for signing
    let payload = LicensePayload {
        version: 1,
        customer_id: customer_id.clone(),
        license_id: license_id.clone(),
        issued_at: issued_at.clone(),
        expires_at: expires_at.clone(),
        not_before: not_before_at.clone(),
        entitlements: entitlements_map,
        watermark: watermark.to_string(),
    };

    // Canonicalize and sign
    let canonical_bytes = canonicalize(&payload).context("Failed to canonicalize payload")?;
    let signature = signing_key.sign(&canonical_bytes);
    let signature_b64 = BASE64.encode(signature.to_bytes());

    // Build full license
    let license = License {
        version: 1,
        customer_id,
        license_id,
        issued_at,
        expires_at,
        not_before: not_before_at,
        entitlements: Entitlements {
            allow_ingest_schemes: schemes,
            allow_video,
            max_session_duration_secs: max_session_secs,
        },
        watermark: watermark.to_string(),
        signature: signature_b64,
    };

    // Write license file
    let license_json =
        serde_json::to_string_pretty(&license).context("Failed to serialize license")?;
    fs::write(output, &license_json).context("Failed to write license file")?;

    println!("License created successfully!");
    println!();
    println!("  Customer:    {}", customer);
    println!("  Customer ID: {}", license.customer_id);
    println!("  License ID:  {}", license.license_id);
    println!("  Expires:     {}", expires);
    println!("  Watermark:   {}", license.watermark);
    println!("  Output:      {}", output.display());
    println!();
    println!("Distribute this license file to the customer.");

    Ok(())
}

fn print_public_key(key_file: &PathBuf) -> Result<()> {
    // Load private key
    let key_b64 = fs::read_to_string(key_file).context("Failed to read private key file")?;
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .context("Failed to decode private key")?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid private key length"))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    let public_bytes = verifying_key.to_bytes();

    println!("// Public key for embedding in examples/cli/stream-health-demo/src/license.rs");
    println!("const EVAL_PUBLIC_KEY: [u8; 32] = [");
    for (i, chunk) in public_bytes.chunks(8).enumerate() {
        print!("    ");
        for (j, byte) in chunk.iter().enumerate() {
            print!("0x{:02x}", byte);
            if i * 8 + j < 31 {
                print!(", ");
            }
        }
        println!();
    }
    println!("];");

    Ok(())
}

fn verify_license(license_file: &PathBuf, key_file: &PathBuf) -> Result<()> {
    // Load private key to derive public key
    let key_b64 = fs::read_to_string(key_file).context("Failed to read private key file")?;
    let key_bytes = BASE64
        .decode(key_b64.trim())
        .context("Failed to decode private key")?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid private key length"))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    // Load license
    let license_json = fs::read_to_string(license_file).context("Failed to read license file")?;
    let license: License =
        serde_json::from_str(&license_json).context("Failed to parse license")?;

    // Rebuild payload for verification
    let mut entitlements_map = BTreeMap::new();
    entitlements_map.insert(
        "allow_ingest_schemes".to_string(),
        serde_json::json!(license.entitlements.allow_ingest_schemes),
    );
    entitlements_map.insert(
        "allow_video".to_string(),
        serde_json::json!(license.entitlements.allow_video),
    );
    if let Some(max_secs) = license.entitlements.max_session_duration_secs {
        entitlements_map.insert(
            "max_session_duration_secs".to_string(),
            serde_json::json!(max_secs),
        );
    }

    let payload = LicensePayload {
        version: license.version,
        customer_id: license.customer_id.clone(),
        license_id: license.license_id.clone(),
        issued_at: license.issued_at.clone(),
        expires_at: license.expires_at.clone(),
        not_before: license.not_before.clone(),
        entitlements: entitlements_map,
        watermark: license.watermark.clone(),
    };

    // Canonicalize
    let canonical_bytes = canonicalize(&payload).context("Failed to canonicalize payload")?;

    // Decode signature
    let sig_bytes = BASE64
        .decode(&license.signature)
        .context("Failed to decode signature")?;
    let sig_bytes: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    // Verify
    match verifying_key.verify_strict(&canonical_bytes, &signature) {
        Ok(()) => {
            println!("License signature is VALID");
            println!();
            println!("  Customer ID: {}", license.customer_id);
            println!("  License ID:  {}", license.license_id);
            println!("  Expires:     {}", license.expires_at);
            println!("  Watermark:   {}", license.watermark);
            Ok(())
        }
        Err(e) => {
            eprintln!("License signature is INVALID: {}", e);
            std::process::exit(1);
        }
    }
}
