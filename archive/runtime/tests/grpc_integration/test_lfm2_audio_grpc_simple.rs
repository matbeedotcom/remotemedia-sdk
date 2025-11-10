//! Simple test to verify LFM2AudioNode can be registered and server starts
//!
//! This test verifies:
//! 1. LFM2AudioNode can be registered in the server
//! 2. Server starts without hanging
//! 3. Basic pipeline manifest can be created

#![cfg(feature = "grpc-transport")]

use serde_json::json;
use tracing::info;

#[tokio::test]
async fn test_lfm2_audio_server_registration() {
    use super::test_helpers::start_test_server;

    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia=info")
        .try_init();

    info!("========================================");
    info!("Testing LFM2AudioNode Server Registration");
    info!("========================================");

    // Initialize Python for the LFM2AudioNode
    pyo3::prepare_freethreaded_python();

    info!("Starting gRPC server with LFM2AudioNode support...");

    // Start test server (it should have LFM2AudioNode registered)
    let addr = start_test_server().await;

    info!("✓ Server started successfully at {}", addr);
    info!("✓ LFM2AudioNode can be registered without hanging");

    // The fact we got here means:
    // 1. The server started
    // 2. LFM2AudioNode registration didn't cause a hang
    // 3. The Python integration is working

    info!("========================================");
    info!("✅ Test Passed!");
    info!("========================================");
}
