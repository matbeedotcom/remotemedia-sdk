# Docker Metrics Collection for Observability

## Overview

The Docker metrics collection system provides comprehensive observability for Docker containers running Python nodes in the RemoteMedia SDK. It collects time-series resource usage data, calculates aggregates, and exposes metrics through a public API.

## Features

- **Time-series metrics collection**: Periodically gathers container statistics including CPU, memory, and network I/O
- **Circular buffer storage**: Efficiently stores metrics in a fixed-size buffer (default: 1000 data points)
- **Aggregated metrics**: Calculate averages, peaks, and minimums over time windows
- **Restart tracking**: Monitor container restart counts for reliability analysis
- **JSON export**: Export metrics for integration with external monitoring tools
- **Low overhead**: Asynchronous collection with configurable intervals

## Architecture

### Components

1. **MetricDataPoint**: Individual time-stamped resource measurement
2. **ContainerMetrics**: Per-container circular buffer storing historical data points
3. **MetricsCollector**: Manages collection for multiple containers with background tasks
4. **AggregatedMetrics**: Statistical summary over a time period
5. **DockerSupport**: Integration layer providing public API

### Data Flow

```
┌──────────────────────────────────────────────────────────┐
│ DockerSupport (Public API)                              │
│  ├─ new_with_metrics()                                   │
│  ├─ start_metrics_collection(container_id)              │
│  ├─ get_container_metrics(container_id, duration)       │
│  └─ export_container_metrics_json(container_id)         │
└────────────────┬─────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────┐
│ MetricsCollector                                         │
│  ├─ Background collection tasks (tokio::spawn)          │
│  ├─ Per-container metrics storage (HashMap)             │
│  └─ Shutdown management (watch channel)                 │
└────────────────┬─────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────┐
│ ContainerMetrics (per container)                         │
│  ├─ Circular buffer (VecDeque<MetricDataPoint>)        │
│  ├─ Restart count tracking                              │
│  └─ Aggregate calculation (avg, peak, min)              │
└────────────────┬─────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────┐
│ Docker Stats API (bollard)                               │
│  ├─ CPU usage (multi-core aware)                        │
│  ├─ Memory usage and limits                             │
│  └─ Network I/O (rx/tx bytes)                           │
└──────────────────────────────────────────────────────────┘
```

## Usage

### Basic Setup

```rust
use remotemedia_runtime_core::python::multiprocess::docker_support::DockerSupport;
use std::time::Duration;

// Create Docker support with metrics collection enabled
let docker_support = DockerSupport::new_with_metrics(
    Some(Duration::from_secs(5)),  // Collect every 5 seconds
    Some(1000),                     // Store up to 1000 data points
).await?;

// Create and start a container
let container_id = docker_support
    .create_container("node_id", "session_id", &config)
    .await?;

docker_support.start_container(&container_id).await?;

// Start collecting metrics
docker_support.start_metrics_collection(&container_id).await?;
```

### Querying Metrics

```rust
// Get aggregated metrics for the last 5 minutes
if let Some(metrics) = docker_support
    .get_container_metrics_last_minutes(&container_id, 5)
    .await
{
    println!("Average CPU: {:.2}%", metrics.avg_cpu_percent);
    println!("Peak Memory: {} MB", metrics.peak_memory_mb);
    println!("Restart Count: {}", metrics.restart_count);
}

// Get recent data points
let recent_points = docker_support
    .get_recent_metric_points(&container_id, 10)
    .await;

for point in recent_points {
    println!("Timestamp: {:?}", point.timestamp);
    println!("  CPU: {:.2}%", point.cpu_percent);
    println!("  Memory: {} MB", point.memory_mb);
}
```

### JSON Export

```rust
// Export all metrics as JSON
if let Some(json) = docker_support
    .export_container_metrics_json(&container_id)
    .await
{
    // JSON includes:
    // - Recent data points (last 100)
    // - Aggregates for 5, 15, and 60 minutes
    // - Container uptime and restart count
    println!("{}", serde_json::to_string_pretty(&json)?);
}
```

### Cleanup

```rust
// Stop metrics collection
docker_support.stop_metrics_collection(&container_id).await?;

// Stop and remove container
docker_support.stop_container(&container_id, Duration::from_secs(5)).await?;
docker_support.remove_container(&container_id, true).await?;
```

## Metrics Reference

### MetricDataPoint

Individual time-stamped measurement:

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `SystemTime` | When the metric was collected |
| `cpu_percent` | `f32` | CPU usage percentage (0-100% per core) |
| `memory_mb` | `u64` | Current memory usage in megabytes |
| `memory_limit_mb` | `Option<u64>` | Memory limit if configured |
| `network_rx_bytes` | `Option<u64>` | Total bytes received |
| `network_tx_bytes` | `Option<u64>` | Total bytes transmitted |
| `uptime_secs` | `u64` | Container uptime in seconds |

### AggregatedMetrics

Statistical summary over a time window:

| Field | Type | Description |
|-------|------|-------------|
| `period_start` | `SystemTime` | Start of aggregation period |
| `period_end` | `SystemTime` | End of aggregation period |
| `sample_count` | `usize` | Number of data points in aggregate |
| `avg_cpu_percent` | `f32` | Average CPU usage |
| `peak_cpu_percent` | `f32` | Peak CPU usage |
| `min_cpu_percent` | `f32` | Minimum CPU usage |
| `avg_memory_mb` | `u64` | Average memory usage |
| `peak_memory_mb` | `u64` | Peak memory usage |
| `min_memory_mb` | `u64` | Minimum memory usage |
| `memory_limit_mb` | `Option<u64>` | Memory limit if configured |
| `total_network_rx_bytes` | `Option<u64>` | Total bytes received (max) |
| `total_network_tx_bytes` | `Option<u64>` | Total bytes transmitted (max) |
| `restart_count` | `u32` | Number of container restarts |

## Configuration

### Collection Interval

Default: 5 seconds

- Faster intervals (1-2s): Higher overhead, more granular data
- Slower intervals (10-30s): Lower overhead, less granular data
- Recommended: 5-10 seconds for production

### Maximum Data Points

Default: 1000 data points per container

Memory usage per container (approximate):
- 1000 points × 80 bytes/point = 80 KB per container
- 10000 points = 800 KB per container

Retention time calculation:
- Collection interval × max_data_points = retention period
- Example: 5s × 1000 = 5000s ≈ 83 minutes

## Performance Considerations

### CPU Overhead

- Stats API call: ~1-2ms per container
- Aggregation: ~100-500μs per query
- Background task: Minimal (sleeps between collections)

### Memory Usage

- Fixed per container based on `max_data_points`
- Circular buffer prevents unbounded growth
- Example: 100 containers × 1000 points = ~8MB total

### Network I/O

- Stats API uses local Docker socket (no network overhead)
- Minimal data transfer (~1KB per stats call)

## Integration with Monitoring Systems

### Prometheus Export

```rust
// Export metrics in Prometheus format
let metrics = docker_support
    .get_container_metrics_last_minutes(&container_id, 1)
    .await?;

println!("# HELP container_cpu_percent Container CPU usage");
println!("# TYPE container_cpu_percent gauge");
println!("container_cpu_percent{{container=\"{}\"}} {}",
    container_id, metrics.avg_cpu_percent);

println!("# HELP container_memory_bytes Container memory usage");
println!("# TYPE container_memory_bytes gauge");
println!("container_memory_bytes{{container=\"{}\"}} {}",
    container_id, metrics.avg_memory_mb * 1_048_576);
```

### Grafana Dashboard

Query recent data points for time-series visualization:

```rust
let points = docker_support
    .get_recent_metric_points(&container_id, 1000)
    .await;

// Convert to Grafana-compatible format
let series = points.iter().map(|point| {
    json!({
        "timestamp": point.timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        "cpu_percent": point.cpu_percent,
        "memory_mb": point.memory_mb,
    })
}).collect::<Vec<_>>();
```

### Custom Alerting

```rust
// Check for resource limit violations
let metrics = docker_support
    .get_container_metrics_last_minutes(&container_id, 5)
    .await?;

// Alert on high memory usage
if let Some(limit) = metrics.memory_limit_mb {
    let usage_percent = (metrics.avg_memory_mb as f32 / limit as f32) * 100.0;
    if usage_percent > 90.0 {
        println!("WARNING: Container {} using {:.1}% of memory limit",
            container_id, usage_percent);
    }
}

// Alert on high CPU usage
if metrics.avg_cpu_percent > 80.0 {
    println!("WARNING: Container {} CPU usage: {:.1}%",
        container_id, metrics.avg_cpu_percent);
}

// Alert on frequent restarts
if metrics.restart_count > 3 {
    println!("WARNING: Container {} restarted {} times",
        container_id, metrics.restart_count);
}
```

## Troubleshooting

### Metrics Not Collected

**Symptom**: `get_recent_metric_points()` returns empty vector

**Solutions**:
1. Verify metrics collection is enabled: `docker_support.is_metrics_enabled()`
2. Ensure `start_metrics_collection()` was called
3. Check container is running: `docker_support.is_container_running()`
4. Wait for first collection interval to pass

### High Memory Usage

**Symptom**: Process memory grows over time

**Solutions**:
1. Reduce `max_data_points` (e.g., from 1000 to 500)
2. Increase `collection_interval` (e.g., from 5s to 10s)
3. Stop collection for unused containers
4. Monitor with: `docker_support.get_monitored_containers()`

### Stats API Errors

**Symptom**: Debug logs show "Failed to collect metrics"

**Solutions**:
1. Container may have stopped (normal, collection continues)
2. Check Docker daemon is responsive
3. Verify container ID is correct
4. Check Docker API permissions

## Best Practices

1. **Enable metrics only when needed**: Metrics collection adds overhead, enable only for monitored containers

2. **Choose appropriate intervals**:
   - Development: 1-2 seconds for debugging
   - Production: 5-10 seconds for balance
   - Long-running: 30-60 seconds for trends

3. **Set reasonable data point limits**:
   - Short-term monitoring: 100-500 points
   - Medium-term: 1000-2000 points
   - Long-term: Use external time-series DB

4. **Clean up after containers**:
   ```rust
   docker_support.stop_metrics_collection(&container_id).await?;
   ```

5. **Export to external systems**:
   - Don't rely solely on in-memory storage
   - Export to Prometheus, InfluxDB, or similar
   - Use JSON export for periodic snapshots

6. **Monitor the monitors**:
   - Track metrics collection overhead
   - Alert on collection failures
   - Monitor memory usage of metrics system

## Example: Complete Monitoring Pipeline

See `/runtime-core/examples/docker_metrics_example.rs` for a complete example demonstrating:

- Creating Docker support with metrics
- Starting container and metrics collection
- Querying recent data points
- Calculating aggregates
- JSON export
- Proper cleanup

Run with:
```bash
cargo run --example docker_metrics_example --features docker
```

## API Reference

See inline documentation in `runtime-core/src/python/multiprocess/docker_support.rs` for detailed API reference.

Key types:
- `DockerSupport::new_with_metrics()` - Initialize with metrics
- `DockerSupport::start_metrics_collection()` - Start collecting
- `DockerSupport::get_container_metrics()` - Get aggregates
- `DockerSupport::export_container_metrics_json()` - JSON export
- `MetricDataPoint` - Individual measurement
- `AggregatedMetrics` - Statistical summary
- `ContainerMetrics` - Per-container storage
- `MetricsCollector` - Multi-container manager
