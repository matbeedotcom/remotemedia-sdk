//! Simple test for process management functionality

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    /// Test that we can spawn multiple Python processes
    #[test]
    fn test_spawn_python_processes() {
        // Test spawning multiple Python processes
        let mut _handles: Vec<()> = Vec::new();

        for i in 0..3 {
            let output = Command::new("python")
                .args(["-c", &format!("import time; print('Process {} started'); time.sleep(0.1); print('Process {} finished')", i, i)])
                .output();

            match output {
                Ok(result) => {
                    let stdout = String::from_utf8_lossy(&result.stdout);
                    println!("Process {}: {}", i, stdout);
                    assert!(stdout.contains(&format!("Process {} started", i)));
                    assert!(stdout.contains(&format!("Process {} finished", i)));
                }
                Err(e) => {
                    eprintln!("Failed to spawn Python process {}: {}", i, e);
                    // Don't fail the test if Python is not available
                    return;
                }
            }
        }

        println!("Successfully spawned and completed {} Python processes", 3);
    }

    /// Test process lifecycle states
    #[test]
    fn test_process_lifecycle() {
        use remotemedia_runtime::python::multiprocess::process_manager::{
            ProcessHandle, ProcessStatus,
        };
        use std::sync::Arc;
        use std::time::Instant;
        use tokio::sync::RwLock;

        // Create a mock process handle
        let handle = ProcessHandle {
            id: 1234,
            node_id: "test_node".to_string(),
            node_type: "test_type".to_string(),
            status: Arc::new(RwLock::new(ProcessStatus::Idle)),
            started_at: Instant::now(),
            inner: Arc::new(tokio::sync::Mutex::new(None)),
        };

        // Test initial state
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            assert_eq!(*handle.status.read().await, ProcessStatus::Idle);

            // Transition through states
            *handle.status.write().await = ProcessStatus::Initializing;
            assert_eq!(*handle.status.read().await, ProcessStatus::Initializing);

            *handle.status.write().await = ProcessStatus::Ready;
            assert_eq!(*handle.status.read().await, ProcessStatus::Ready);

            *handle.status.write().await = ProcessStatus::Processing;
            assert_eq!(*handle.status.read().await, ProcessStatus::Processing);

            *handle.status.write().await = ProcessStatus::Stopping;
            assert_eq!(*handle.status.read().await, ProcessStatus::Stopping);

            *handle.status.write().await = ProcessStatus::Stopped;
            assert_eq!(*handle.status.read().await, ProcessStatus::Stopped);
        });

        println!("Process lifecycle states work correctly");
    }

    /// Test health monitoring
    #[test]
    fn test_health_monitoring() {
        use remotemedia_runtime::python::multiprocess::health_monitor::{
            HealthMonitor, ProcessHealthStats,
        };
        use std::time::Instant;

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let monitor = HealthMonitor::new(30);

            // Add health stats for a process
            let stats = ProcessHealthStats {
                uptime: Duration::from_secs(60),
                cpu_usage: 25.5,
                memory_usage: 1024 * 1024 * 100, // 100MB
                restart_count: 0,
                last_check: Some(Instant::now()),
                is_responsive: true,
            };

            monitor.update_health_stats(1234, stats.clone()).await;

            // Retrieve and verify stats
            let retrieved = monitor.get_health_stats(1234).await;
            assert!(retrieved.is_some());

            let retrieved_stats = retrieved.unwrap();
            assert_eq!(retrieved_stats.cpu_usage, 25.5);
            assert_eq!(retrieved_stats.memory_usage, 1024 * 1024 * 100);
            assert!(retrieved_stats.is_responsive);

            // Check health
            assert!(monitor.is_healthy(1234).await);
        });

        println!("Health monitoring works correctly");
    }

    /// Test concurrent execution simulation
    #[test]
    fn test_concurrent_execution() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Instant;

        // Simulate concurrent execution of multiple nodes
        let start = Instant::now();
        let counter = Arc::new(Mutex::new(0));
        let mut handles = vec![];

        for i in 0..3 {
            let counter_clone = Arc::clone(&counter);
            let handle = thread::spawn(move || {
                // Simulate some work
                thread::sleep(Duration::from_millis(50));
                let mut num = counter_clone.lock().unwrap();
                *num += 1;
                println!("Node {} completed", i);
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let elapsed = start.elapsed();
        let final_count = *counter.lock().unwrap();

        assert_eq!(final_count, 3, "All nodes should have completed");
        assert!(
            elapsed < Duration::from_millis(200),
            "Concurrent execution should be faster than sequential (took {:?})",
            elapsed
        );

        println!("Concurrent execution completed in {:?}", elapsed);
    }
}
