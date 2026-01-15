//! Integration tests for Docker image building and caching (T031)

#[cfg(all(feature = "docker", feature = "multiprocess"))]
mod tests {
    use bollard::Docker;
    use remotemedia_core::python::multiprocess::container_builder::ContainerBuilder;
    use remotemedia_core::python::multiprocess::docker_support::DockerNodeConfig;
    use std::sync::Arc;

    /// T031: Test image building and caching functionality
    #[tokio::test]
    async fn test_docker_image_building_and_caching() {
        // Skip if Docker is not available
        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => Arc::new(d),
            Err(_) => {
                eprintln!("Docker not available, skipping test");
                return;
            }
        };

        // Verify Docker daemon is running
        match docker.ping().await {
            Ok(_) => {}
            Err(_) => {
                eprintln!("Docker daemon not running, skipping test");
                return;
            }
        }

        // Create container builder with 100MB cache
        let builder = ContainerBuilder::new(docker, Some(100 * 1024 * 1024));

        // Test configuration
        let config = DockerNodeConfig {
            python_version: "3.10".to_string(),
            python_packages: vec![
                "numpy==1.24.0".to_string(),
                "requests==2.31.0".to_string(), // Use a package that definitely exists
            ],
            system_packages: vec![],
            memory_mb: 512, // Minimum required
            cpu_cores: 0.5,
            base_image: None,
            shm_size_mb: 512,
            env_vars: std::collections::HashMap::new(),
            gpu_devices: vec![],
            volumes: vec![],
            security: Default::default(),
        };

        // First build - should build from scratch
        let image1 = builder.build_image(&config, false).await;
        if let Err(ref e) = image1 {
            eprintln!("Image build failed: {:?}", e);
        }
        assert!(
            image1.is_ok(),
            "First image build should succeed: {:?}",
            image1.err()
        );
        let image1 = image1.unwrap();

        // Verify image properties
        assert!(!image1.image_tag.is_empty());
        assert!(!image1.config_hash.is_empty());
        assert_eq!(image1.python_version, "3.10");

        // Second build with same config - should use cache
        let image2 = builder.build_image(&config, false).await;
        assert!(image2.is_ok(), "Second image build should succeed");
        let image2 = image2.unwrap();

        // Should have same hash and tag (cached)
        assert_eq!(image1.config_hash, image2.config_hash);
        assert_eq!(image1.image_tag, image2.image_tag);

        // Third build with force_rebuild - should rebuild
        let image3 = builder.build_image(&config, true).await;
        assert!(image3.is_ok(), "Force rebuild should succeed");
        let image3 = image3.unwrap();

        // Should have same hash but new build
        assert_eq!(image1.config_hash, image3.config_hash);

        // Test with different config - should build new image
        let config2 = DockerNodeConfig {
            python_packages: vec![
                "scipy==1.10.0".to_string(), // Different packages
                "pandas==2.0.0".to_string(),
            ],
            ..config.clone()
        };

        let image4 = builder.build_image(&config2, false).await;
        assert!(image4.is_ok(), "Different config build should succeed");
        let image4 = image4.unwrap();

        // Should have different hash
        assert_ne!(image1.config_hash, image4.config_hash);
        assert_ne!(image1.image_tag, image4.image_tag);

        // Check cache statistics
        let (count, _size, max_size) = builder.cache_stats().await;
        assert!(count >= 2, "Cache should have at least 2 images");
        // Note: size might be 0 if we don't query Docker for actual image sizes
        // This is acceptable for the test as we're mainly testing caching logic
        assert!(max_size > 0, "Max cache size should be configured");

        // Test cache clearing
        builder.clear_cache().await;
        let (count_after, _, _) = builder.cache_stats().await;
        assert_eq!(count_after, 0, "Cache should be empty after clearing");
    }

    /// Test Dockerfile generation (T023)
    #[tokio::test]
    async fn test_dockerfile_generation() {
        use remotemedia_core::python::multiprocess::container_builder::ContainerBuilder;

        let config = DockerNodeConfig {
            python_version: "3.10".to_string(),
            python_packages: vec![
                "numpy==1.24.0".to_string(),
                "torch>=2.0.0".to_string(),
                "iceoryx2".to_string(),
            ],
            system_packages: vec!["ffmpeg".to_string(), "libsndfile1".to_string()],
            memory_mb: 1024,
            cpu_cores: 1.5,
            base_image: Some("python:3.10-slim".to_string()),
            shm_size_mb: 2048,
            env_vars: {
                let mut env = std::collections::HashMap::new();
                env.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());
                env.insert("MODEL_PATH".to_string(), "/models".to_string());
                env
            },
            gpu_devices: vec![],
            volumes: vec![],
            security: Default::default(),
        };

        // Generate Dockerfile
        let dockerfile = ContainerBuilder::generate_dockerfile(&config);
        assert!(dockerfile.is_ok(), "Dockerfile generation should succeed");
        let dockerfile = dockerfile.unwrap();

        // Verify Dockerfile contains expected sections
        assert!(dockerfile.contains("FROM python:3.10-slim"));
        assert!(dockerfile.contains("apt-get install"));
        assert!(dockerfile.contains("ffmpeg"));
        assert!(dockerfile.contains("libsndfile1"));
        assert!(dockerfile.contains("pip install"));
        assert!(dockerfile.contains("numpy==1.24.0"));
        assert!(dockerfile.contains("torch>=2.0.0"));
        assert!(dockerfile.contains("iceoryx2"));
        assert!(dockerfile.contains("ENV PYTHONUNBUFFERED"));
        assert!(dockerfile.contains("ENV MODEL_PATH=\"/models\""));
        assert!(dockerfile.contains("WORKDIR /app"));
        assert!(dockerfile.contains("CMD [\"python\", \"-c\""));
    }

    /// Test config hash generation (T022)
    #[test]
    fn test_config_hash_generation() {
        use remotemedia_core::python::multiprocess::container_builder::ContainerBuilder;

        let config1 = DockerNodeConfig {
            python_version: "3.10".to_string(),
            python_packages: vec!["numpy".to_string()],
            system_packages: vec![],
            memory_mb: 512,
            cpu_cores: 1.0,
            base_image: None,
            shm_size_mb: 1024,
            env_vars: std::collections::HashMap::new(),
            gpu_devices: vec![],
            volumes: vec![],
            security: Default::default(),
        };

        let config2 = config1.clone();
        let config3 = DockerNodeConfig {
            python_packages: vec!["scipy".to_string()], // Different package
            ..config1.clone()
        };

        // Same config should produce same hash
        let hash1 = ContainerBuilder::compute_config_hash(&config1);
        let hash2 = ContainerBuilder::compute_config_hash(&config2);
        assert_eq!(hash1, hash2, "Same config should produce same hash");

        // Different config should produce different hash
        let hash3 = ContainerBuilder::compute_config_hash(&config3);
        assert_ne!(
            hash1, hash3,
            "Different config should produce different hash"
        );

        // Hash should be hex string of consistent length (SHA256 = 64 chars)
        assert_eq!(hash1.len(), 64, "Hash should be 64 characters (SHA256)");
        assert!(
            hash1.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hex string"
        );
    }
}
