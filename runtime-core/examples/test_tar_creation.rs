//! Test program to debug tar archive creation for Docker builds
//!
//! This helps identify problematic files causing "unhandled tar header type 83" errors
//!
//! Usage:
//!   cargo run --example test_tar_creation --features docker

use remotemedia_runtime_core::python::docker::config::{DockerExecutorConfig, ResourceLimits};
use std::path::PathBuf;

fn main() {
    println!("=== Testing Docker Tar Archive Creation ===\n");

    // Create a test config
    let config = DockerExecutorConfig {
        python_version: "3.11".to_string(),
        system_dependencies: vec![],
        python_packages: vec!["iceoryx2".to_string()],
        resource_limits: ResourceLimits {
            memory_mb: 512,
            cpu_cores: 1.0,
        },
        base_image: None,
        env: Default::default(),
    };

    println!("1. Generating Dockerfile...");
    let dockerfile =
        match remotemedia_runtime_core::python::docker::image_builder::generate_dockerfile(&config)
        {
            Ok(df) => {
                println!("✓ Dockerfile generated ({} bytes)", df.len());
                df
            }
            Err(e) => {
                eprintln!("✗ Failed to generate Dockerfile: {}", e);
                return;
            }
        };

    println!("\n2. Creating tar archive...");

    // Call the private function via the public API
    // We'll create the tar the same way build_docker_image does
    let mut tar_data = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut tar_data);

        // Add Dockerfile
        let dockerfile_bytes = dockerfile.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(dockerfile_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        if let Err(e) = tar.append_data(&mut header, "Dockerfile", dockerfile_bytes) {
            eprintln!("✗ Failed to add Dockerfile: {}", e);
            return;
        }
        println!("  ✓ Added Dockerfile");

        // Find workspace root
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to find workspace root")
            .to_path_buf();

        println!("  Workspace root: {}", workspace_root.display());

        // Add remotemedia shared package
        let remotemedia_shared = workspace_root.join("remotemedia");
        if remotemedia_shared.exists() {
            println!("\n  Adding remotemedia directory...");
            tar.follow_symlinks(false);

            match tar.append_dir_all("remotemedia", &remotemedia_shared) {
                Ok(_) => {
                    println!("  ✓ Added remotemedia directory");
                }
                Err(e) => {
                    eprintln!("  ✗ FAILED to add remotemedia: {}", e);
                    eprintln!("     Error type: {:?}", e);
                    return;
                }
            }
        }

        // Add root files
        let root_setup = workspace_root.join("setup.py");
        if root_setup.exists() {
            match tar.append_path_with_name(&root_setup, "setup.py") {
                Ok(_) => println!("  ✓ Added setup.py"),
                Err(e) => eprintln!("  ✗ Failed to add setup.py: {}", e),
            }
        }

        let root_readme = workspace_root.join("README.md");
        if root_readme.exists() {
            match tar.append_path_with_name(&root_readme, "README.md") {
                Ok(_) => println!("  ✓ Added README.md"),
                Err(e) => eprintln!("  ✗ Failed to add README.md: {}", e),
            }
        }

        // Add python-client directory (this is where the error likely occurs)
        let python_client_path = workspace_root.join("python-client");
        if python_client_path.exists() {
            println!("\n  Adding python-client directory...");
            println!("    Path: {}", python_client_path.display());

            tar.follow_symlinks(false);

            match tar.append_dir_all("python-client", &python_client_path) {
                Ok(_) => {
                    println!("  ✓ Added python-client directory");
                }
                Err(e) => {
                    eprintln!("  ✗ FAILED to add python-client: {}", e);
                    eprintln!("     Error type: {:?}", e);
                    eprintln!("\n  This is likely the source of 'tar header type 83' error");
                    eprintln!("  Socket files may exist in:");
                    eprintln!("    - python-client/.pytest_cache/");
                    eprintln!("    - python-client/__pycache__/");
                    eprintln!("\n  Try: find python-client -type s");
                    return;
                }
            }
        }

        // Finalize tar
        match tar.finish() {
            Ok(_) => {
                println!("\n✓ Tar archive finalized successfully");
            }
            Err(e) => {
                eprintln!("\n✗ Failed to finalize tar: {}", e);
                return;
            }
        }
    }

    println!("\n3. Tar archive summary:");
    println!("  Total size: {} bytes", tar_data.len());
    println!(
        "  Size (MB): {:.2}",
        tar_data.len() as f64 / 1024.0 / 1024.0
    );

    // Optionally save to file for inspection
    let tar_path = "/tmp/docker_build_test.tar";
    if let Err(e) = std::fs::write(tar_path, &tar_data) {
        eprintln!("  Failed to save tar file: {}", e);
    } else {
        println!("\n✓ Tar file saved to: {}", tar_path);
        println!("\nInspect with:");
        println!("  tar -tvf {}", tar_path);
        println!("  tar -xf {} -C /tmp/test-extract", tar_path);
    }

    println!("\n✅ Tar creation test completed successfully!");
}
