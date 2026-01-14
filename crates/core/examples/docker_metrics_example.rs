//! Docker metrics collection example
//!
//! Demonstrates how to use the Docker metrics collection system for observability.
//!
//! Run with:
//! ```bash
//! cargo run --example docker_metrics_example --features docker
//! ```

use remotemedia_core::python::multiprocess::docker_support::{
    AggregatedMetrics, DockerNodeConfig, DockerSupport,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("Docker Metrics Collection Example");
    println!("==================================\n");

    // Create Docker support with metrics collection enabled
    // Collect metrics every 2 seconds, store up to 500 data points per container
    let docker_support =
        DockerSupport::new_with_metrics(Some(Duration::from_secs(2)), Some(500)).await?;

    println!("✓ Docker support initialized with metrics collection");
    println!("  Collection interval: 2 seconds");
    println!("  Max data points per container: 500\n");

    // Create a simple test container configuration
    let config = DockerNodeConfig {
        python_version: "3.11".to_string(),
        base_image: None,
        system_packages: vec![],
        python_packages: vec![],
        memory_mb: 512,
        cpu_cores: 1.0,
        gpu_devices: vec![],
        shm_size_mb: 256,
        env_vars: std::collections::HashMap::new(),
        volumes: vec![],
        security: Default::default(),
    };

    // Create and start a container
    println!("Creating test container...");
    let container_id = docker_support
        .create_container("test_node", "test_session", &config)
        .await?;

    println!("✓ Container created: {}", container_id);

    // Start the container
    docker_support.start_container(&container_id).await?;
    println!("✓ Container started\n");

    // Start metrics collection for this container
    docker_support
        .start_metrics_collection(&container_id)
        .await?;

    println!("✓ Metrics collection started for container\n");
    println!("Collecting metrics for 30 seconds...\n");

    // Collect metrics for 30 seconds
    for i in 1..=15 {
        sleep(Duration::from_secs(2)).await;

        // Get recent data points
        let recent_points = docker_support
            .get_recent_metric_points(&container_id, 5)
            .await;

        if let Some(latest) = recent_points.first() {
            println!(
                "Sample {}: CPU: {:.2}%, Memory: {} MB / {} MB",
                i,
                latest.cpu_percent,
                latest.memory_mb,
                latest.memory_limit_mb.unwrap_or(0)
            );
        }
    }

    println!("\n--- Metrics Summary ---\n");

    // Get aggregated metrics for the last 1 minute
    if let Some(metrics) = docker_support
        .get_container_metrics_last_minutes(&container_id, 1)
        .await
    {
        print_aggregated_metrics(&metrics);
    } else {
        println!("No metrics available");
    }

    // Export metrics as JSON
    println!("\n--- JSON Export (recent points) ---\n");
    if let Some(json) = docker_support
        .export_container_metrics_json(&container_id)
        .await
    {
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    // Cleanup
    println!("\n--- Cleanup ---\n");
    docker_support
        .stop_metrics_collection(&container_id)
        .await?;
    println!("✓ Metrics collection stopped");

    docker_support
        .stop_container(&container_id, Duration::from_secs(5))
        .await?;
    println!("✓ Container stopped");

    docker_support.remove_container(&container_id, true).await?;
    println!("✓ Container removed");

    println!("\nExample completed successfully!");

    Ok(())
}

fn print_aggregated_metrics(metrics: &AggregatedMetrics) {
    println!("Time period: {:?} samples", metrics.sample_count);
    println!("\nCPU Usage:");
    println!("  Average: {:.2}%", metrics.avg_cpu_percent);
    println!("  Peak:    {:.2}%", metrics.peak_cpu_percent);
    println!("  Min:     {:.2}%", metrics.min_cpu_percent);

    println!("\nMemory Usage:");
    println!("  Average: {} MB", metrics.avg_memory_mb);
    println!("  Peak:    {} MB", metrics.peak_memory_mb);
    println!("  Min:     {} MB", metrics.min_memory_mb);
    if let Some(limit) = metrics.memory_limit_mb {
        println!("  Limit:   {} MB", limit);
        println!(
            "  Utilization: {:.1}%",
            (metrics.avg_memory_mb as f32 / limit as f32) * 100.0
        );
    }

    if let (Some(rx), Some(tx)) = (
        metrics.total_network_rx_bytes,
        metrics.total_network_tx_bytes,
    ) {
        println!("\nNetwork I/O:");
        println!("  RX: {} bytes ({:.2} MB)", rx, rx as f64 / 1_048_576.0);
        println!("  TX: {} bytes ({:.2} MB)", tx, tx as f64 / 1_048_576.0);
    }

    println!("\nContainer restarts: {}", metrics.restart_count);
}
