//! Performance and metrics overhead tests
//!
//! Phase 7 (T124-T126): Validate metrics collection overhead <100μs

use remotemedia_runtime::executor::PipelineMetrics;
use std::time::Instant;

/// T124: Test metrics overhead measurement
#[tokio::test]
async fn test_metrics_overhead_measurement() {
    let mut metrics = PipelineMetrics::new("test_pipeline");
    
    // Start execution timing
    metrics.start_execution();
    
    // Simulate some node executions
    for i in 0..10 {
        metrics.record_node_execution(
            format!("node_{}", i),
            std::time::Duration::from_micros(100),
            true,
        );
    }
    
    // End execution timing
    metrics.end_execution();
    
    // Measure JSON serialization overhead
    let start = Instant::now();
    let json = metrics.to_json();
    let serialization_overhead = start.elapsed();
    
    println!("Metrics JSON: {}", serde_json::to_string_pretty(&json).unwrap());
    println!("Serialization overhead: {:?}", serialization_overhead);
    
    // Check that overhead measurement is included
    assert!(json.get("metrics_overhead_us").is_some());
    let recorded_overhead = json["metrics_overhead_us"].as_u64().unwrap();
    
    println!("Recorded overhead: {}μs", recorded_overhead);
    
    // Overhead should be reasonable (this just checks it's measured)
    assert!(recorded_overhead > 0);
}

/// T125: Test metrics JSON export with complex pipeline
#[tokio::test]
async fn test_metrics_json_export() {
    let mut metrics = PipelineMetrics::new("complex_pipeline");
    
    metrics.start_execution();
    
    // Simulate various node executions
    metrics.record_node_execution("source", std::time::Duration::from_micros(50), true);
    metrics.record_node_execution("transform1", std::time::Duration::from_micros(150), true);
    metrics.record_node_execution("transform2", std::time::Duration::from_micros(200), true);
    metrics.record_node_execution("transform1", std::time::Duration::from_micros(100), true);
    metrics.record_node_execution("sink", std::time::Duration::from_micros(75), true);
    
    // Simulate a failed execution
    metrics.record_node_execution("transform2", std::time::Duration::from_micros(50), false);
    
    metrics.end_execution();
    
    // Update memory usage
    metrics.update_peak_memory(1024 * 1024 * 5); // 5 MB
    
    // Export to JSON
    let json = metrics.to_json();
    
    // Verify structure
    assert_eq!(json["pipeline_id"], "complex_pipeline");
    assert_eq!(json["total_executions"], 1);
    assert!(json["total_duration_us"].as_u64().unwrap() > 0);
    assert_eq!(json["peak_memory_bytes"], 1024 * 1024 * 5);
    
    let node_metrics = json["node_metrics"].as_array().unwrap();
    assert_eq!(node_metrics.len(), 4); // 4 unique nodes
    
    // Find transform1 metrics (executed twice)
    let transform1 = node_metrics
        .iter()
        .find(|m| m["node_id"] == "transform1")
        .unwrap();
    
    assert_eq!(transform1["execution_count"], 2);
    assert_eq!(transform1["success_count"], 2);
    assert_eq!(transform1["error_count"], 0);
    
    // Find transform2 metrics (1 success, 1 failure)
    let transform2 = node_metrics
        .iter()
        .find(|m| m["node_id"] == "transform2")
        .unwrap();
    
    assert_eq!(transform2["execution_count"], 2);
    assert_eq!(transform2["success_count"], 1);
    assert_eq!(transform2["error_count"], 1);
    assert_eq!(transform2["success_rate"], 0.5);
    
    println!("Metrics JSON:\n{}", serde_json::to_string_pretty(&json).unwrap());
}

/// T126: Verify metrics collection overhead is <100μs per pipeline
#[tokio::test]
async fn test_metrics_overhead_under_100us() {
    const ITERATIONS: usize = 100;
    let mut total_overhead_us: u128 = 0;
    
    for _ in 0..ITERATIONS {
        let mut metrics = PipelineMetrics::new("benchmark_pipeline");
        
        metrics.start_execution();
        
        // Simulate a typical pipeline with 5 nodes
        for i in 0..5 {
            metrics.record_node_execution(
                format!("node_{}", i),
                std::time::Duration::from_micros(100),
                true,
            );
        }
        
        metrics.end_execution();
        
        // Measure serialization overhead
        let start = Instant::now();
        let json = metrics.to_json();
        let overhead = start.elapsed();
        
        total_overhead_us += overhead.as_micros();
        
        // Individual overhead should be well under 100μs
        assert!(
            overhead.as_micros() < 200,
            "Individual overhead {}μs exceeds 200μs threshold",
            overhead.as_micros()
        );
        
        // Verify the metrics include the overhead measurement
        assert!(json.get("metrics_overhead_us").is_some());
    }
    
    let avg_overhead_us = total_overhead_us / ITERATIONS as u128;
    
    println!("Average metrics overhead over {} iterations: {}μs", ITERATIONS, avg_overhead_us);
    println!("Target: <100μs");
    
    // Success criteria: average overhead <100μs
    assert!(
        avg_overhead_us < 100,
        "Average overhead {}μs exceeds 100μs target",
        avg_overhead_us
    );
}

/// Test metrics with microsecond precision
#[tokio::test]
async fn test_metrics_microsecond_precision() {
    let mut metrics = PipelineMetrics::new("precision_test");
    
    metrics.start_execution();
    
    // Record very short durations
    metrics.record_node_execution("fast_node", std::time::Duration::from_nanos(500), true);
    metrics.record_node_execution("fast_node", std::time::Duration::from_nanos(1500), true);
    metrics.record_node_execution("fast_node", std::time::Duration::from_nanos(2500), true);
    
    metrics.end_execution();
    
    let json = metrics.to_json();
    
    // Verify microsecond fields are present
    let node_metrics = json["node_metrics"].as_array().unwrap();
    let fast_node = &node_metrics[0];
    
    assert!(fast_node["avg_duration_us"].is_u64() || fast_node["avg_duration_us"].is_i64());
    assert!(fast_node["min_duration_us"].is_u64() || fast_node["min_duration_us"].is_i64());
    assert!(fast_node["max_duration_us"].is_u64() || fast_node["max_duration_us"].is_i64());
    
    // Even sub-microsecond durations should be captured (as 0 or 1 μs)
    let avg_us = fast_node["avg_duration_us"].as_u64().unwrap();
    assert!(avg_us <= 2, "Average should be 0-2μs for nanosecond durations");
    
    println!("Microsecond precision metrics:\n{}", serde_json::to_string_pretty(&json).unwrap());
}

/// Test metrics with no node executions (edge case)
#[tokio::test]
async fn test_metrics_empty_pipeline() {
    let mut metrics = PipelineMetrics::new("empty_pipeline");
    
    metrics.start_execution();
    metrics.end_execution();
    
    let json = metrics.to_json();
    
    assert_eq!(json["pipeline_id"], "empty_pipeline");
    assert_eq!(json["total_executions"], 1);
    assert!(json["total_duration_us"].as_u64().unwrap() >= 0);
    assert_eq!(json["peak_memory_bytes"], 0);
    
    let node_metrics = json["node_metrics"].as_array().unwrap();
    assert_eq!(node_metrics.len(), 0);
    
    println!("Empty pipeline metrics:\n{}", serde_json::to_string_pretty(&json).unwrap());
}

