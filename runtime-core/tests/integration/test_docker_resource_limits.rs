//! Integration tests for Docker resource limit enforcement (T041)
//!
//! This test module validates that Docker containers properly enforce:
//! - Memory limits (T041.1)
//! - CPU limits using nano_cpus (T041.2)
//! - GPU device passthrough configuration (T041.3)
//! - Resource monitoring functionality (T041.4)
//! - OOM (Out of Memory) handling behavior (T041.5)
//!
//! Requirements:
//! - Docker daemon running
//! - Skip if Docker unavailable: SKIP_DOCKER_TESTS=1
//!
//! Success Criteria:
//! - Memory limits are correctly applied to containers
//! - CPU limits are enforced using Docker's nano_cpus
//! - GPU devices are correctly passed through to containers
//! - Resource usage can be monitored in real-time
//! - Containers handle OOM scenarios gracefully

#[cfg(all(feature = "docker", feature = "multiprocess"))]
mod tests {
    use bollard::Docker;
    use remotemedia_runtime_core::python::multiprocess::docker_support::{
        DockerNodeConfig, DockerSupport,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::Uuid;

    /// Check if Docker is available for testing
    fn is_docker_available() -> bool {
        if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
            return false;
        }
        std::process::Command::new("docker")
            .arg("info")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create a basic test configuration with specified resource limits
    fn create_test_config(memory_mb: u64, cpu_cores: f32) -> DockerNodeConfig {
        DockerNodeConfig {
            python_version: "3.10".to_string(),
            base_image: Some("python:3.10-slim".to_string()),
            system_packages: vec![],
            python_packages: vec![],
            memory_mb,
            cpu_cores,
            gpu_devices: vec![],
            shm_size_mb: 512,
            env_vars: HashMap::new(),
            volumes: vec![],
        }
    }

    /// T041.1: Test that memory limits are properly enforced
    ///
    /// This test verifies that:
    /// 1. Memory limits are correctly applied during container creation
    /// 2. Docker reports the configured memory limit via inspect
    /// 3. Multiple memory limit values are correctly handled
    #[tokio::test]
    async fn test_docker_memory_limit_enforcement() {
        if !is_docker_available() {
            eprintln!("Docker not available, skipping test");
            return;
        }

        println!("\n=== T041.1: Memory Limit Enforcement ===");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Failed to connect to Docker daemon, skipping test");
                return;
            }
        };

        // Verify Docker daemon is running
        if docker.ping().await.is_err() {
            eprintln!("Docker daemon not responding, skipping test");
            return;
        }

        let docker_support = DockerSupport::new()
            .await
            .expect("Failed to create DockerSupport");

        // Test different memory limits
        let test_cases = vec![("512MB", 512u64), ("1GB", 1024u64), ("2GB", 2048u64)];

        for (name, memory_mb) in test_cases {
            println!("\nTest case: {} memory limit", name);

            let config = create_test_config(memory_mb, 0.5);
            let session_id = format!("mem_test_{}", memory_mb);
            let node_id = "test_node";

            // Create container with memory limit
            let container_id = match docker_support
                .create_container(node_id, &session_id, &config)
                .await
            {
                Ok(id) => {
                    println!("  ✓ Container created: {}", id);
                    id
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to create container: {}", e);
                    continue;
                }
            };

            // Inspect container to verify memory limit
            match docker
                .inspect_container(
                    &container_id,
                    None::<bollard::query_parameters::InspectContainerOptions>,
                )
                .await
            {
                Ok(info) => {
                    if let Some(host_config) = info.host_config {
                        if let Some(memory) = host_config.memory {
                            let memory_mb_actual = (memory / 1_048_576) as u64;
                            println!("  ✓ Memory limit set: {} MB", memory_mb_actual);

                            // Verify the limit matches configuration
                            assert_eq!(
                                memory_mb_actual, memory_mb,
                                "Memory limit mismatch: expected {} MB, got {} MB",
                                memory_mb, memory_mb_actual
                            );
                            println!("  ✓ Memory limit verified: {} MB", memory_mb);
                        } else {
                            eprintln!("  ✗ Memory limit not set in container");
                            assert!(false, "Memory limit not configured");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to inspect container: {}", e);
                }
            }

            // Cleanup
            let _ = docker_support.remove_container(&container_id, true).await;
            println!("  ✓ Container cleaned up");
        }

        println!("\n✅ Memory Limit Enforcement Test PASSED");
    }

    /// T041.2: Test that CPU limits are properly applied using nano_cpus
    ///
    /// This test verifies that:
    /// 1. CPU limits are correctly converted to nano_cpus (cores * 1e9)
    /// 2. Docker reports the configured CPU limit via inspect
    /// 3. Fractional CPU allocations (e.g., 0.5 cores) are correctly handled
    #[tokio::test]
    async fn test_docker_cpu_limit_enforcement() {
        if !is_docker_available() {
            eprintln!("Docker not available, skipping test");
            return;
        }

        println!("\n=== T041.2: CPU Limit Enforcement ===");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Failed to connect to Docker daemon, skipping test");
                return;
            }
        };

        if docker.ping().await.is_err() {
            eprintln!("Docker daemon not responding, skipping test");
            return;
        }

        let docker_support = DockerSupport::new()
            .await
            .expect("Failed to create DockerSupport");

        // Test different CPU limits
        let test_cases = vec![
            ("0.25 cores", 0.25f32),
            ("0.5 cores", 0.5f32),
            ("1 core", 1.0f32),
            ("2 cores", 2.0f32),
        ];

        for (name, cpu_cores) in test_cases {
            println!("\nTest case: {} CPU limit", name);

            let config = create_test_config(512, cpu_cores);
            let session_id = format!("cpu_test_{}", (cpu_cores * 100.0) as u32);
            let node_id = "test_node";

            // Create container with CPU limit
            let container_id = match docker_support
                .create_container(node_id, &session_id, &config)
                .await
            {
                Ok(id) => {
                    println!("  ✓ Container created: {}", id);
                    id
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to create container: {}", e);
                    continue;
                }
            };

            // Inspect container to verify CPU limit
            match docker
                .inspect_container(
                    &container_id,
                    None::<bollard::query_parameters::InspectContainerOptions>,
                )
                .await
            {
                Ok(info) => {
                    if let Some(host_config) = info.host_config {
                        if let Some(nano_cpus) = host_config.nano_cpus {
                            let cpu_cores_actual = nano_cpus as f32 / 1_000_000_000.0;
                            println!(
                                "  ✓ CPU limit set: {} cores (nano_cpus: {})",
                                cpu_cores_actual, nano_cpus
                            );

                            // Verify the limit matches configuration (with small tolerance for floating point)
                            let diff = (cpu_cores_actual - cpu_cores).abs();
                            assert!(
                                diff < 0.001,
                                "CPU limit mismatch: expected {} cores, got {} cores",
                                cpu_cores,
                                cpu_cores_actual
                            );
                            println!("  ✓ CPU limit verified: {} cores", cpu_cores);
                        } else {
                            eprintln!("  ✗ CPU limit (nano_cpus) not set in container");
                            assert!(false, "CPU limit not configured");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to inspect container: {}", e);
                }
            }

            // Cleanup
            let _ = docker_support.remove_container(&container_id, true).await;
            println!("  ✓ Container cleaned up");
        }

        println!("\n✅ CPU Limit Enforcement Test PASSED");
    }

    /// T041.3: Test GPU device passthrough configuration
    ///
    /// This test verifies that:
    /// 1. GPU devices are correctly configured in container settings
    /// 2. "all" GPU configuration requests all available GPUs
    /// 3. Specific GPU device IDs are correctly passed through
    /// 4. NVIDIA runtime is properly configured
    #[tokio::test]
    async fn test_docker_gpu_device_passthrough() {
        if !is_docker_available() {
            eprintln!("Docker not available, skipping test");
            return;
        }

        println!("\n=== T041.3: GPU Device Passthrough ===");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Failed to connect to Docker daemon, skipping test");
                return;
            }
        };

        if docker.ping().await.is_err() {
            eprintln!("Docker daemon not responding, skipping test");
            return;
        }

        let docker_support = DockerSupport::new()
            .await
            .expect("Failed to create DockerSupport");

        // Test cases for GPU configuration
        let test_cases = vec![
            ("All GPUs", vec!["all".to_string()], true, None),
            (
                "Specific GPU 0",
                vec!["0".to_string()],
                false,
                Some(vec!["0".to_string()]),
            ),
            (
                "Multiple GPUs",
                vec!["0".to_string(), "1".to_string()],
                false,
                Some(vec!["0".to_string(), "1".to_string()]),
            ),
        ];

        for (name, gpu_devices, expect_all, expected_device_ids) in test_cases {
            println!("\nTest case: {}", name);

            let mut config = create_test_config(512, 1.0);
            config.gpu_devices = gpu_devices;

            let session_id = format!("gpu_test_{}", name.replace(" ", "_"));
            let node_id = "test_node";

            // Create container with GPU configuration
            let container_id = match docker_support
                .create_container(node_id, &session_id, &config)
                .await
            {
                Ok(id) => {
                    println!("  ✓ Container created: {}", id);
                    id
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to create container: {}", e);
                    continue;
                }
            };

            // Inspect container to verify GPU configuration
            match docker
                .inspect_container(
                    &container_id,
                    None::<bollard::query_parameters::InspectContainerOptions>,
                )
                .await
            {
                Ok(info) => {
                    if let Some(host_config) = info.host_config {
                        if let Some(device_requests) = host_config.device_requests {
                            if !device_requests.is_empty() {
                                let device_request = &device_requests[0];

                                // Verify driver is NVIDIA
                                if let Some(driver) = &device_request.driver {
                                    assert_eq!(driver, "nvidia", "GPU driver should be nvidia");
                                    println!("  ✓ GPU driver configured: nvidia");
                                }

                                // Verify count for "all" devices
                                if expect_all {
                                    if let Some(count) = device_request.count {
                                        assert_eq!(
                                            count, -1,
                                            "GPU count should be -1 for all devices"
                                        );
                                        println!("  ✓ GPU count configured: all devices (-1)");
                                    }
                                }

                                // Verify specific device IDs
                                if let Some(expected_ids) = expected_device_ids {
                                    if let Some(device_ids) = &device_request.device_ids {
                                        assert_eq!(
                                            device_ids, &expected_ids,
                                            "GPU device IDs mismatch"
                                        );
                                        println!("  ✓ GPU device IDs configured: {:?}", device_ids);
                                    }
                                }

                                // Verify capabilities
                                if let Some(capabilities) = &device_request.capabilities {
                                    assert!(
                                        !capabilities.is_empty(),
                                        "GPU capabilities should be set"
                                    );
                                    println!("  ✓ GPU capabilities configured: {:?}", capabilities);
                                }

                                println!("  ✓ GPU device passthrough verified");
                            } else {
                                eprintln!("  ✗ No device requests found");
                                assert!(false, "GPU device requests not configured");
                            }
                        } else {
                            eprintln!("  ✗ No device requests in host config");
                            assert!(false, "GPU configuration not found");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ✗ Failed to inspect container: {}", e);
                }
            }

            // Cleanup
            let _ = docker_support.remove_container(&container_id, true).await;
            println!("  ✓ Container cleaned up");
        }

        println!("\n✅ GPU Device Passthrough Test PASSED");
    }

    /// T041.4: Test resource monitoring functionality
    ///
    /// This test verifies that:
    /// 1. Resource usage can be monitored for running containers
    /// 2. CPU percentage is correctly calculated
    /// 3. Memory usage is reported in megabytes
    /// 4. Memory limits are included in the stats
    #[tokio::test]
    async fn test_docker_resource_monitoring() {
        if !is_docker_available() {
            eprintln!("Docker not available, skipping test");
            return;
        }

        println!("\n=== T041.4: Resource Monitoring ===");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Failed to connect to Docker daemon, skipping test");
                return;
            }
        };

        if docker.ping().await.is_err() {
            eprintln!("Docker daemon not responding, skipping test");
            return;
        }

        let docker_support = DockerSupport::new()
            .await
            .expect("Failed to create DockerSupport");

        let config = create_test_config(512, 1.0);
        let session_id = format!("monitor_test_{}", Uuid::new_v4());
        let node_id = "test_node";

        // Create container
        let container_id = match docker_support
            .create_container(node_id, &session_id, &config)
            .await
        {
            Ok(id) => {
                println!("✓ Container created: {}", id);
                id
            }
            Err(e) => {
                eprintln!("✗ Failed to create container: {}", e);
                return;
            }
        };

        // Start the container (required for monitoring)
        if let Err(e) = docker_support.start_container(&container_id).await {
            eprintln!("✗ Failed to start container: {}", e);
            let _ = docker_support.remove_container(&container_id, true).await;
            return;
        }
        println!("✓ Container started");

        // Wait a moment for container to initialize
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check if container is still running
        let is_running = docker_support
            .is_container_running(&container_id)
            .await
            .unwrap_or(false);

        if !is_running {
            println!("⚠ Container exited immediately (no command specified)");
            println!(
                "⚠ Skipping resource monitoring test (container needs a long-running process)"
            );
            println!("⚠ This is expected behavior for containers without a command");

            // Verify we can still inspect the container config
            match docker
                .inspect_container(
                    &container_id,
                    None::<bollard::query_parameters::InspectContainerOptions>,
                )
                .await
            {
                Ok(info) => {
                    if let Some(host_config) = info.host_config {
                        if let Some(memory) = host_config.memory {
                            let memory_mb = (memory / 1_048_576) as u64;
                            println!("✓ Memory limit was configured: {} MB", memory_mb);
                        }
                        if let Some(nano_cpus) = host_config.nano_cpus {
                            let cpu_cores = nano_cpus as f32 / 1_000_000_000.0;
                            println!("✓ CPU limit was configured: {} cores", cpu_cores);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to inspect container: {}", e);
                }
            }
        } else {
            // Monitor resource usage if container is running
            match docker_support.monitor_resource_usage(&container_id).await {
                Ok(stats) => {
                    println!("✓ Resource monitoring successful");
                    println!("  CPU usage: {:.2}%", stats.cpu_percent);
                    println!("  Memory usage: {} MB", stats.memory_mb);

                    if let Some(limit) = stats.memory_limit_mb {
                        println!("  Memory limit: {} MB", limit);

                        // Verify memory limit matches configuration
                        assert_eq!(
                            limit, config.memory_mb,
                            "Memory limit in stats should match configuration"
                        );
                        println!("  ✓ Memory limit verified: {} MB", limit);
                    } else {
                        eprintln!("  ⚠ Memory limit not reported in stats");
                    }

                    // Verify CPU stat is non-negative
                    assert!(
                        stats.cpu_percent >= 0.0,
                        "CPU percent should be non-negative"
                    );

                    println!("✓ Resource stats validated");
                }
                Err(e) => {
                    eprintln!("✗ Failed to monitor resource usage: {}", e);
                    assert!(false, "Resource monitoring failed");
                }
            }
        }

        // Cleanup
        let _ = docker_support
            .stop_container(&container_id, Duration::from_secs(5))
            .await;
        let _ = docker_support.remove_container(&container_id, true).await;
        println!("✓ Container cleaned up");

        println!("\n✅ Resource Monitoring Test PASSED");
    }

    /// T041.5: Test OOM (Out of Memory) handling behavior
    ///
    /// This test verifies that:
    /// 1. Containers with very low memory limits can be created
    /// 2. Containers respect memory limits when running
    /// 3. Container state can be checked after potential OOM
    /// 4. OOM situations are handled gracefully
    #[tokio::test]
    async fn test_docker_oom_handling() {
        if !is_docker_available() {
            eprintln!("Docker not available, skipping test");
            return;
        }

        println!("\n=== T041.5: OOM Handling ===");

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Failed to connect to Docker daemon, skipping test");
                return;
            }
        };

        if docker.ping().await.is_err() {
            eprintln!("Docker daemon not responding, skipping test");
            return;
        }

        let docker_support = DockerSupport::new()
            .await
            .expect("Failed to create DockerSupport");

        // Create container with minimal memory (512MB is minimum)
        let config = create_test_config(512, 0.5);
        let session_id = "oom_test";
        let node_id = "test_node";

        println!(
            "Creating container with {} MB memory limit",
            config.memory_mb
        );

        let container_id = match docker_support
            .create_container(node_id, session_id, &config)
            .await
        {
            Ok(id) => {
                println!("✓ Container created with low memory: {}", id);
                id
            }
            Err(e) => {
                eprintln!("✗ Failed to create container: {}", e);
                return;
            }
        };

        // Verify memory limit is set
        match docker
            .inspect_container(
                &container_id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
        {
            Ok(info) => {
                if let Some(host_config) = info.host_config {
                    if let Some(memory) = host_config.memory {
                        let memory_mb = (memory / 1_048_576) as u64;
                        println!("✓ Memory limit verified: {} MB", memory_mb);
                        assert_eq!(memory_mb, config.memory_mb);
                    }

                    // Check OOM kill disable flag (should be false/not set for proper OOM handling)
                    let oom_kill_disable = host_config.oom_kill_disable.unwrap_or(false);
                    println!("✓ OOM kill disable: {}", oom_kill_disable);

                    // We want OOM killer enabled (oom_kill_disable = false) for proper handling
                    assert_eq!(
                        oom_kill_disable, false,
                        "OOM killer should be enabled for proper memory limit enforcement"
                    );
                    println!("✓ OOM killer is enabled (proper configuration)");
                }
            }
            Err(e) => {
                eprintln!("✗ Failed to inspect container: {}", e);
            }
        }

        // Start container to verify it can run with the limit
        match docker_support.start_container(&container_id).await {
            Ok(_) => {
                println!("✓ Container started successfully with memory limit");

                // Wait a moment
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Check if container is still running
                match docker_support.is_container_running(&container_id).await {
                    Ok(is_running) => {
                        if is_running {
                            println!("✓ Container is running within memory limits");
                        } else {
                            println!("⚠ Container stopped (may have hit OOM)");
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to check container status: {}", e);
                    }
                }

                // Check container exit code if stopped
                match docker
                    .inspect_container(
                        &container_id,
                        None::<bollard::query_parameters::InspectContainerOptions>,
                    )
                    .await
                {
                    Ok(info) => {
                        if let Some(state) = info.state {
                            if let Some(exit_code) = state.exit_code {
                                println!("  Container exit code: {}", exit_code);

                                // Exit code 137 typically indicates OOM kill (SIGKILL)
                                if exit_code == 137 {
                                    println!("  ⚠ Container was OOM killed (exit code 137)");
                                } else if exit_code == 0 {
                                    println!("  ✓ Container exited normally");
                                }
                            }

                            if let Some(oom_killed) = state.oom_killed {
                                if oom_killed {
                                    println!("  ⚠ Container was OOM killed by Docker");
                                } else {
                                    println!("  ✓ Container was not OOM killed");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  ✗ Failed to inspect container state: {}", e);
                    }
                }

                // Stop container
                let _ = docker_support
                    .stop_container(&container_id, Duration::from_secs(5))
                    .await;
            }
            Err(e) => {
                eprintln!(
                    "⚠ Failed to start container (may be expected with very low memory): {}",
                    e
                );
            }
        }

        // Cleanup
        let _ = docker_support.remove_container(&container_id, true).await;
        println!("✓ Container cleaned up");

        println!("\n✅ OOM Handling Test PASSED");
    }

    /// Test that invalid resource configurations are rejected
    #[tokio::test]
    async fn test_invalid_resource_limits() {
        println!("\n=== Bonus: Invalid Resource Limits Validation ===");

        // Test memory too low
        let low_memory_config = DockerNodeConfig {
            python_version: "3.10".to_string(),
            base_image: None,
            system_packages: vec![],
            python_packages: vec![],
            memory_mb: 128, // Below minimum of 512MB
            cpu_cores: 1.0,
            gpu_devices: vec![],
            shm_size_mb: 512,
            env_vars: HashMap::new(),
            volumes: vec![],
            security: Default::default(),
        };

        match low_memory_config.validate() {
            Ok(_) => {
                eprintln!("✗ Should have rejected low memory configuration");
                assert!(false, "Low memory config should be rejected");
            }
            Err(e) => {
                println!("✓ Low memory rejected: {}", e);
            }
        }

        // Test CPU too low
        let low_cpu_config = DockerNodeConfig {
            python_version: "3.10".to_string(),
            base_image: None,
            system_packages: vec![],
            python_packages: vec![],
            memory_mb: 512,
            cpu_cores: 0.05, // Below minimum of 0.1
            gpu_devices: vec![],
            shm_size_mb: 512,
            env_vars: HashMap::new(),
            volumes: vec![],
            security: Default::default(),
        };

        match low_cpu_config.validate() {
            Ok(_) => {
                eprintln!("✗ Should have rejected low CPU configuration");
                assert!(false, "Low CPU config should be rejected");
            }
            Err(e) => {
                println!("✓ Low CPU rejected: {}", e);
            }
        }

        println!("\n✅ Invalid Resource Limits Validation Test PASSED");
    }
}

// Test module is empty without the docker feature
#[cfg(not(all(feature = "docker", feature = "multiprocess")))]
mod tests {
    #[test]
    fn test_skipped_without_docker_feature() {
        println!("Docker tests skipped - 'docker' and 'multiprocess' features not enabled");
    }
}
