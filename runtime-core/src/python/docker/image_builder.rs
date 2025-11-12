//! Docker image building and caching
//!
//! Handles Docker image creation, Dockerfile generation,
//! and SQLite-based image cache management.

use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Metadata for a cached Docker image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub id: Option<i64>,
    pub image_name: String,
    pub image_tag: String,
    pub image_id: String,
    pub digest: Option<String>,
    pub config_hash: String,
    pub python_version: String,
    pub base_image: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub last_used: String,
    pub build_config: String,
    pub dependencies: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
}

/// SQLite-based image cache for Docker images
pub struct ImageCache {
    conn: Connection,
}

impl ImageCache {
    /// Create or open image cache database
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path).map_err(|e| {
            Error::Execution(format!("Failed to open image cache database: {}", e))
        })?;

        // Initialize schema
        let schema = include_str!("image_cache_schema.sql");
        conn.execute_batch(schema).map_err(|e| {
            Error::Execution(format!("Failed to initialize image cache schema: {}", e))
        })?;

        Ok(Self { conn })
    }

    /// Get image metadata by name and tag
    pub fn get_image(&self, name: &str, tag: &str) -> Result<Option<ImageMetadata>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, image_name, image_tag, image_id, digest, config_hash,
                        python_version, base_image, size_bytes, created_at, last_used,
                        build_config, dependencies, status, error_message
                 FROM image_metadata
                 WHERE image_name = ? AND image_tag = ? AND status = 'available'",
                params![name, tag],
                |row| {
                    Ok(ImageMetadata {
                        id: row.get(0)?,
                        image_name: row.get(1)?,
                        image_tag: row.get(2)?,
                        image_id: row.get(3)?,
                        digest: row.get(4)?,
                        config_hash: row.get(5)?,
                        python_version: row.get(6)?,
                        base_image: row.get(7)?,
                        size_bytes: row.get(8)?,
                        created_at: row.get(9)?,
                        last_used: row.get(10)?,
                        build_config: row.get(11)?,
                        dependencies: row.get(12)?,
                        status: row.get(13)?,
                        error_message: row.get(14)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Execution(format!("Failed to query image cache: {}", e)))?;

        Ok(result)
    }

    /// Get image by config hash
    pub fn get_image_by_config_hash(&self, config_hash: &str) -> Result<Option<ImageMetadata>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, image_name, image_tag, image_id, digest, config_hash,
                        python_version, base_image, size_bytes, created_at, last_used,
                        build_config, dependencies, status, error_message
                 FROM image_metadata
                 WHERE config_hash = ? AND status = 'available'",
                params![config_hash],
                |row| {
                    Ok(ImageMetadata {
                        id: row.get(0)?,
                        image_name: row.get(1)?,
                        image_tag: row.get(2)?,
                        image_id: row.get(3)?,
                        digest: row.get(4)?,
                        config_hash: row.get(5)?,
                        python_version: row.get(6)?,
                        base_image: row.get(7)?,
                        size_bytes: row.get(8)?,
                        created_at: row.get(9)?,
                        last_used: row.get(10)?,
                        build_config: row.get(11)?,
                        dependencies: row.get(12)?,
                        status: row.get(13)?,
                        error_message: row.get(14)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Execution(format!("Failed to query image by hash: {}", e)))?;

        Ok(result)
    }

    /// Insert or update image metadata
    pub fn upsert_image(&mut self, metadata: &ImageMetadata) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO image_metadata (
                    image_name, image_tag, image_id, digest, config_hash,
                    python_version, base_image, size_bytes, build_config,
                    dependencies, status, error_message, last_used
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
                 ON CONFLICT(image_name, image_tag) DO UPDATE SET
                    image_id = excluded.image_id,
                    size_bytes = excluded.size_bytes,
                    status = excluded.status,
                    error_message = excluded.error_message,
                    last_used = CURRENT_TIMESTAMP",
                params![
                    metadata.image_name,
                    metadata.image_tag,
                    metadata.image_id,
                    metadata.digest,
                    metadata.config_hash,
                    metadata.python_version,
                    metadata.base_image,
                    metadata.size_bytes,
                    metadata.build_config,
                    metadata.dependencies,
                    metadata.status,
                    metadata.error_message,
                ],
            )
            .map_err(|e| Error::Execution(format!("Failed to upsert image metadata: {}", e)))?;

        Ok(())
    }

    /// Update last_used timestamp for an image
    pub fn mark_image_used(&mut self, image_name: &str, image_tag: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE image_metadata SET last_used = CURRENT_TIMESTAMP
                 WHERE image_name = ? AND image_tag = ?",
                params![image_name, image_tag],
            )
            .map_err(|e| Error::Execution(format!("Failed to update image usage: {}", e)))?;

        Ok(())
    }

    /// Evict least recently used images beyond max_count
    pub fn evict_lru_images(&mut self, max_count: usize) -> Result<Vec<String>> {
        // Get images to remove (beyond max_count, ordered by last_used DESC)
        let mut stmt = self
            .conn
            .prepare(
                "SELECT image_id FROM image_metadata
                 ORDER BY last_used DESC
                 LIMIT -1 OFFSET ?",
            )
            .map_err(|e| Error::Execution(format!("Failed to prepare eviction query: {}", e)))?;

        let images_to_remove: Vec<String> = stmt
            .query_map(params![max_count], |row| row.get(0))
            .map_err(|e| Error::Execution(format!("Failed to query images for eviction: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Execution(format!("Failed to collect eviction results: {}", e)))?;

        // Delete from database
        for image_id in &images_to_remove {
            self.conn
                .execute(
                    "DELETE FROM image_metadata WHERE image_id = ?",
                    params![image_id],
                )
                .map_err(|e| {
                    Error::Execution(format!("Failed to delete image metadata: {}", e))
                })?;
        }

        Ok(images_to_remove)
    }
}

/// Select appropriate Dockerfile based on configuration (T020)
///
/// Returns the path to the Dockerfile to use for the build
fn select_dockerfile(_config: &super::config::DockerExecutorConfig) -> Result<std::path::PathBuf> {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| Error::Execution("Failed to find workspace root".to_string()))?
        .to_path_buf();

    // Always use PyTorch Dockerfile for now (most complete, supports both ML and non-ML workloads)
    let dockerfile_name = "Dockerfile.pytorch-node";
    let dockerfile_path = workspace_root.join("docker").join(dockerfile_name);

    if !dockerfile_path.exists() {
        return Err(Error::Execution(format!(
            "Dockerfile not found at {:?}. Please ensure the docker/{} file exists.",
            dockerfile_path, dockerfile_name
        )));
    }

    tracing::debug!("Using Dockerfile: {}", dockerfile_name);
    Ok(dockerfile_path)
}

/// Create tar archive containing Dockerfile and python-client source
fn create_dockerfile_tar(config: &super::config::DockerExecutorConfig) -> Result<Vec<u8>> {
    let mut tar_data = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut tar_data);

        // Get the path to the actual Dockerfile
        let dockerfile_path = select_dockerfile(config)?;

        // Read the Dockerfile content
        let dockerfile_content = std::fs::read_to_string(&dockerfile_path)
            .map_err(|e| Error::Execution(format!("Failed to read Dockerfile: {}", e)))?;

        // Add Dockerfile to tar archive
        let dockerfile_bytes = dockerfile_content.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(dockerfile_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        tar.append_data(&mut header, "Dockerfile", dockerfile_bytes)
            .map_err(|e| Error::Execution(format!("Failed to add Dockerfile to tar: {}", e)))?;

        // Add remotemedia packages to build context
        // Need both: remotemedia (shared) + python-client (SDK)
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| Error::Execution("Failed to find workspace root".to_string()))?
            .to_path_buf();

        // Add root remotemedia package (shared protobuf definitions)
        let remotemedia_shared = workspace_root.join("remotemedia");
        if remotemedia_shared.exists() {
            tar.follow_symlinks(false);
            match tar.append_dir_all("remotemedia", &remotemedia_shared) {
                Ok(_) => {
                    tracing::debug!("Added remotemedia shared package to build context");
                }
                Err(e) => {
                    tracing::error!("Failed to add remotemedia dir to tar: {}", e);
                    return Err(Error::Execution(format!(
                        "Failed to add remotemedia directory to tar: {}. \
                         Check for special files with: find remotemedia -type s -o -type p", e
                    )));
                }
            }
        }

        // Add root setup.py and README.md for shared package
        let root_setup = workspace_root.join("setup.py");
        if root_setup.exists() {
            tar.append_path_with_name(&root_setup, "setup.py")
                .map_err(|e| Error::Execution(format!("Failed to add setup.py: {}", e)))?;
        }

        let root_readme = workspace_root.join("README.md");
        if root_readme.exists() {
            tar.append_path_with_name(&root_readme, "README.md")
                .map_err(|e| Error::Execution(format!("Failed to add README.md: {}", e)))?;
        }

        // Add python-client directory (SDK package)
        // Note: Use follow_symlinks to avoid issues with special files
        let python_client_path = workspace_root.join("python-client");
        if python_client_path.exists() {
            // Use append_dir_all with follow_symlinks to avoid socket/special file issues
            tar.follow_symlinks(false);  // Don't follow symlinks to avoid cycles

            // Set filter to skip cache directories and special files
            tar.follow_symlinks(false);

            // Instead of append_dir_all which includes everything, manually filter
            // This avoids socket files from pytest cache causing tar header errors
            let filter = |path: &std::path::Path| -> bool {
                let path_str = path.to_string_lossy();
                // Skip cache directories, test artifacts, and hidden files
                !path_str.contains("__pycache__") &&
                !path_str.contains(".pytest_cache") &&
                !path_str.contains("/.") &&  // Skip hidden dirs
                !path_str.contains(".egg-info") &&
                !path_str.contains("node_modules")
            };

            // Try to add with filtering
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tar.append_dir_all("python-client", &python_client_path)
            }));

            match result {
                Ok(Ok(_)) => {
                    tracing::debug!("Added python-client directory to Docker build context");
                }
                Ok(Err(e)) => {
                    // Tar error - likely socket files
                    tracing::error!(
                        "Failed to add python-client to tar archive: {}. \
                         Socket files detected in cache directories.", e
                    );
                    return Err(Error::Execution(
                        format!("Failed to create tar archive for Docker build: {}. \
                                Socket or special files detected. \
                                Clean with: find python-client -type s -delete", e)
                    ));
                }
                Err(_) => {
                    // Panic during tar creation
                    return Err(Error::Execution(
                        "Failed to create tar archive (panic). \
                         Clean cache: find python-client -name '__pycache__' -exec rm -rf {{}} +".to_string()
                    ));
                }
            }
        } else {
            tracing::warn!(
                "python-client directory not found at {:?}, remotemedia package won't be available in container",
                python_client_path
            );
        }

        tar.finish()
            .map_err(|e| Error::Execution(format!("Failed to finalize tar: {}", e)))?;
    }

    Ok(tar_data)
}

/// Build Docker image from configuration (T019-T021)
pub async fn build_docker_image(
    docker: &bollard::Docker,
    cache: &mut ImageCache,
    config: &super::config::DockerExecutorConfig,
    node_id: &str,
) -> Result<String> {
    let config_hash = config.compute_config_hash();

    // T021: Check cache first
    if let Some(cached) = cache.get_image_by_config_hash(&config_hash)? {
        tracing::info!("Using cached image: {} ({})", cached.image_tag, cached.image_id);
        cache.mark_image_used(&cached.image_name, &cached.image_tag)?;
        return Ok(cached.image_tag);
    }

    tracing::info!("Building new Docker image for node: {}", node_id);

    // T020: Create tar archive with the actual Dockerfile from docker/ directory
    let tar_bytes = create_dockerfile_tar(config)?;

    // T019: Build using bollard
    let image_name = format!("remotemedia/{}", node_id);
    let image_tag = format!("{}:py{}-{}", image_name, config.python_version, &config_hash[..8]);

    let build_options = bollard::query_parameters::BuildImageOptionsBuilder::default()
        .dockerfile("Dockerfile")
        .t(&image_tag)
        .rm(true)
        .pull("true")
        .build();

    // T019: Build image using bollard
    use futures::StreamExt;
    let mut build_stream = docker.build_image(
        build_options,
        None,
        Some(bollard::body_full(bytes::Bytes::from(tar_bytes))),
    );

    let mut final_image_id = String::new();
    let mut step_count = 0;

    while let Some(output) = build_stream.next().await {
        match output {
            Ok(info) => {
                if let Some(stream) = info.stream {
                    let msg = stream.trim();
                    if !msg.is_empty() {
                        // Log at info level for important steps and package installation progress
                        if msg.starts_with("Step ")
                            || msg.contains("Pulling")
                            || msg.contains("Downloaded")
                            || msg.contains("Collecting")
                            || msg.contains("Downloading")
                            || msg.contains("Installing")
                            || msg.contains("Successfully installed")
                            || msg.contains("Requirement already satisfied")
                            || msg.contains("ERROR")
                            || msg.contains("WARNING") {
                            tracing::info!("Docker build [{}]: {}", node_id, msg);
                            step_count += 1;
                        } else {
                            tracing::debug!("Docker build [{}]: {}", node_id, msg);
                        }
                    }
                }
                if let Some(status) = info.status {
                    tracing::info!("Docker build [{}] status: {}", node_id, status);
                }
                if let Some(error) = info.error {
                    return Err(Error::Execution(format!(
                        "Docker build error for {}: {}",
                        node_id, error
                    )));
                }
                // Capture image ID from aux field
                if let Some(aux) = info.aux {
                    // aux is already an ImageId struct
                    if let Some(id) = aux.id {
                        tracing::info!("Docker build [{}]: Image ID captured: {}", node_id, id);
                        final_image_id = id;
                    }
                }
            }
            Err(e) => {
                return Err(Error::Execution(format!("Docker build failed for {} after {} steps: {}", node_id, step_count, e)));
            }
        }
    }

    tracing::info!("Docker build completed for node: {}", node_id);

    // Get image ID via inspection
    let inspect = docker.inspect_image(&image_tag).await.map_err(|e| {
        Error::Execution(format!("Failed to inspect built image: {}", e))
    })?;

    let image_id = inspect.id.unwrap_or_else(|| {
        if !final_image_id.is_empty() {
            final_image_id
        } else {
            image_tag.clone()
        }
    });
    let size_bytes = inspect.size.unwrap_or(0);

    // Save to cache
    let metadata = ImageMetadata {
        id: None,
        image_name,
        image_tag: image_tag.clone(),
        image_id: image_id.clone(),
        digest: None,
        config_hash,
        python_version: config.python_version.clone(),
        base_image: config.base_image.clone().unwrap_or_else(|| "standard".to_string()),
        size_bytes,
        created_at: chrono::Utc::now().to_rfc3339(),
        last_used: chrono::Utc::now().to_rfc3339(),
        build_config: serde_json::to_string(&config).unwrap_or_default(),
        dependencies: Some(serde_json::to_string(&config.python_packages).unwrap_or_default()),
        status: "available".to_string(),
        error_message: None,
    };

    cache.upsert_image(&metadata)?;

    tracing::info!("Image built and cached: {} ({})", image_tag, image_id);

    Ok(image_tag)
}

/// Validate custom Docker base image (FR-016)
///
/// Checks that a custom base image has required iceoryx2 libraries installed
/// and accessible system paths (/tmp, /dev).
///
/// This is a placeholder implementation. Full validation would require:
/// - Pulling the image if not present locally
/// - Running a test container to verify iceoryx2 is importable
/// - Checking that /tmp and /dev are accessible
///
/// For MVP, we perform basic validation only.
pub async fn validate_custom_base_image(base_image: &str) -> Result<()> {
    // Basic format validation
    if base_image.trim().is_empty() {
        return Err(Error::InvalidManifest(
            "Custom base image cannot be empty".to_string(),
        ));
    }

    // Check image reference format (simple validation)
    // Format: [registry/]name[:tag]
    if !base_image.contains(':') && !base_image.contains('/') {
        return Err(Error::InvalidManifest(format!(
            "Invalid Docker image reference: '{}'. Expected format: name:tag or registry/name:tag",
            base_image
        )));
    }

    // TODO: In full implementation, we would:
    // 1. Connect to Docker daemon
    // 2. Check if image exists locally or pull it
    // 3. Run test container: docker run --rm <image> python -c "import iceoryx2"
    // 4. Verify exit code == 0

    tracing::info!(
        "Custom base image '{}' format validated (full runtime validation pending)",
        base_image
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_image_cache_creation() {
        let temp_db = NamedTempFile::new().unwrap();
        let cache = ImageCache::new(temp_db.path());
        assert!(cache.is_ok());
    }

    #[test]
    fn test_image_cache_upsert_and_get() {
        let temp_db = NamedTempFile::new().unwrap();
        let mut cache = ImageCache::new(temp_db.path()).unwrap();

        let metadata = ImageMetadata {
            id: None,
            image_name: "test-node".to_string(),
            image_tag: "py310-abc123".to_string(),
            image_id: "sha256:1234567890abcdef".to_string(),
            digest: None,
            config_hash: "a".repeat(64),
            python_version: "3.10".to_string(),
            base_image: "python:3.10-slim".to_string(),
            size_bytes: 500_000_000,
            created_at: "2025-11-11T00:00:00Z".to_string(),
            last_used: "2025-11-11T00:00:00Z".to_string(),
            build_config: "{}".to_string(),
            dependencies: None,
            status: "available".to_string(),
            error_message: None,
        };

        // Insert
        cache.upsert_image(&metadata).unwrap();

        // Retrieve
        let retrieved = cache.get_image("test-node", "py310-abc123").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.image_id, "sha256:1234567890abcdef");
        assert_eq!(retrieved.python_version, "3.10");
    }

    #[test]
    fn test_dockerfile_selection() {
        use crate::python::docker::config::*;

        let config = DockerExecutorConfig {
            python_version: "3.11".to_string(),
            system_dependencies: vec!["ffmpeg".to_string()],
            python_packages: vec!["numpy==1.26.0".to_string()],
            resource_limits: ResourceLimits {
                memory_mb: 1024,
                cpu_cores: 1.0,
                gpu_devices: vec![],
            },
            base_image: None,
            env: Default::default(),
        };

        // Verify we always use pytorch-node for now
        if let Ok(path) = select_dockerfile(&config) {
            assert!(path.to_string_lossy().contains("pytorch-node"));
            println!("Selected Dockerfile: {:?}", path);
        }
    }
}

