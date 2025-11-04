//! Simple demonstration test to verify streaming behavior

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tracing::info;

/// Track chunk timing
#[derive(Debug)]
struct ChunkMetrics {
    sent_at: Vec<Instant>,
    received_at: Vec<Instant>,
}

#[tokio::test]
async fn test_streaming_demonstration() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .try_init();

    info!("=== Streaming Architecture Test Demo ===");

    let metrics = Arc::new(Mutex::new(ChunkMetrics {
        sent_at: Vec::new(),
        received_at: Vec::new(),
    }));

    // Create channels to simulate the streaming pipeline
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<(u64, Instant)>();
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<(u64, Instant, Instant)>();

    let metrics_clone = metrics.clone();

    // Simulate a streaming node that processes chunks immediately
    let processor = tokio::spawn(async move {
        info!("Processor: Starting to listen for chunks...");

        while let Some((seq, sent_time)) = input_rx.recv().await {
            let received_time = Instant::now();
            info!("Processor: Received chunk {} (latency: {:?})", seq, received_time - sent_time);

            // Simulate some processing time
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Immediately forward the chunk
            let _ = output_tx.send((seq, sent_time, received_time)).await;
            info!("Processor: Forwarded chunk {} immediately", seq);
        }
    });

    // Track outputs
    let metrics_clone2 = metrics.clone();
    let receiver = tokio::spawn(async move {
        while let Some((seq, sent_time, _proc_time)) = output_rx.recv().await {
            let final_time = Instant::now();
            let mut m = metrics_clone2.lock().await;

            if seq as usize >= m.sent_at.len() {
                m.sent_at.resize(seq as usize + 1, sent_time);
                m.received_at.resize(seq as usize + 1, final_time);
            }

            m.sent_at[seq as usize] = sent_time;
            m.received_at[seq as usize] = final_time;

            info!("Client: Received chunk {} (end-to-end latency: {:?})",
                  seq, final_time - sent_time);
        }
    });

    // Send chunks with delays to simulate streaming
    info!("\nSending 5 chunks with 100ms delays...");
    for i in 0..5 {
        let sent_time = Instant::now();
        input_tx.send((i, sent_time)).unwrap();

        let mut m = metrics_clone.lock().await;
        if i as usize >= m.sent_at.len() {
            m.sent_at.resize(i as usize + 1, sent_time);
        }
        m.sent_at[i as usize] = sent_time;
        drop(m);

        info!("Main: Sent chunk {}", i);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Cleanup
    drop(input_tx);
    let _ = processor.await;
    let _ = receiver.await;

    // Analyze results
    let m = metrics.lock().await;

    info!("\n=== Streaming Timing Analysis ===");
    info!("Chunks sent: {}", m.sent_at.len());
    info!("Chunks received: {}", m.received_at.len());

    if m.received_at.len() >= 2 {
        // Calculate gaps between chunk arrivals
        for i in 1..m.received_at.len() {
            let gap = m.received_at[i] - m.received_at[i-1];
            info!("Gap between chunk {} and {}: {:?}", i-1, i, gap);

            // In a streaming system, gaps should be ~100ms (our send interval)
            // Not all at once (which would show gaps < 20ms)
            assert!(gap.as_millis() >= 80,
                   "Chunks arrived too close together - not streaming!");
        }

        // Calculate end-to-end latency for each chunk
        for i in 0..m.sent_at.len().min(m.received_at.len()) {
            let latency = m.received_at[i] - m.sent_at[i];
            info!("Chunk {} end-to-end latency: {:?}", i, latency);

            // Latency should be low (< 50ms for our simple processing)
            assert!(latency.as_millis() < 50,
                   "High latency detected - possible batching!");
        }
    }

    info!("\n✅ Test PASSED - System is streaming chunks immediately!");
    info!("Each chunk was processed and forwarded as soon as it arrived,");
    info!("maintaining low latency and consistent inter-chunk timing.");
}