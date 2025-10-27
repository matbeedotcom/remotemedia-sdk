//! CLI tool for detecting node capabilities via build trials
//!
//! Usage:
//!   cargo run --bin detect_capabilities -- <path-to-node-cargo-toml>
//!
//! Example:
//!   cargo run --bin detect_capabilities -- ./test_nodes/simple_node/Cargo.toml

use remotemedia_runtime::capabilities::detect_and_print;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <path-to-node-file>", args[0]);
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  {} ./test_nodes/simple_node/Cargo.toml", args[0]);
        eprintln!("  {} ./nodes/whisper_node.py", args[0]);
        std::process::exit(1);
    }

    let node_path = &args[1];

    match detect_and_print(node_path) {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
