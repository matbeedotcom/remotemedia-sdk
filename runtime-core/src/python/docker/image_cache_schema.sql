-- SQLite schema for Docker image cache
-- Spec 009: Docker-based node execution
-- Tracks built images for reuse across pipeline sessions

CREATE TABLE IF NOT EXISTS image_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Image identifiers
    image_name TEXT NOT NULL,
    image_tag TEXT NOT NULL,
    image_id TEXT NOT NULL,  -- Docker image ID (sha256:...)
    digest TEXT,              -- Manifest digest for verification

    -- Configuration tracking
    config_hash TEXT NOT NULL,  -- SHA256 of DockerExecutorConfig
    python_version TEXT NOT NULL,
    base_image TEXT NOT NULL,

    -- Metadata
    size_bytes INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_pulled TIMESTAMP,
    last_used TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    -- Build configuration (JSON serialized)
    build_config TEXT NOT NULL,    -- DockerExecutorConfig as JSON
    dependencies TEXT,              -- Combined system_deps + python_packages as JSON

    -- Status tracking
    status TEXT NOT NULL CHECK(status IN ('available', 'building', 'failed')),
    error_message TEXT,             -- If status='failed', store error details

    -- Unique constraint
    UNIQUE(image_name, image_tag)
);

-- Index for fast lookups by name/tag
CREATE INDEX IF NOT EXISTS idx_image_lookup
    ON image_metadata(image_name, image_tag);

-- Index for LRU eviction
CREATE INDEX IF NOT EXISTS idx_last_used
    ON image_metadata(last_used DESC);

-- Index for filtering by status
CREATE INDEX IF NOT EXISTS idx_status
    ON image_metadata(status);

-- Index for config hash lookups
CREATE INDEX IF NOT EXISTS idx_config_hash
    ON image_metadata(config_hash);
