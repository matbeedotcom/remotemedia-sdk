//! Integration test for Docker executor with iceoryx2 IPC

use remotemedia_runtime_core::python::docker::{
    config::{DockerExecutorConfig, DockerizedNodeConfiguration, ResourceLimits},
    docker_executor::DockerExecutor,
};
use std::time::Instant;

fn is_docker_available() -> bool {
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return false;
    }
    use std::process::Command;
    Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn test_docker_executor_creation() {
    if !is_docker_available() {
        println!("Skipping test: Docker not available");
        return;
    }
    let config = DockerizedNodeConfiguration::new_without_type(
        "test_node".to_string(),
        DockerExecutorConfig {
            python_version: "3.10".to_string(),
            system_dependencies: vec![],
            python_packages: vec!["iceoryx2".to_string()],
            resource_limits: ResourceLimits {
                memory_mb: 512,
                cpu_cores: 0.5,
                gpu_devices: vec![],
            },
            base_image: None,
            env: Default::default(),
        },
    );
    let result = DockerExecutor::new(config, None);
    assert!(
        result.is_ok(),
        "Docker executor creation should succeed when Docker is available"
    );
}
