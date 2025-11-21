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
        let conn = Connection::open(db_path)
            .map_err(|e| Error::Execution(format!("Failed to open image cache database: {}", e)))?;

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
                .map_err(|e| Error::Execution(format!("Failed to delete image metadata: {}", e)))?;
        }

        Ok(images_to_remove)
    }
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
}
