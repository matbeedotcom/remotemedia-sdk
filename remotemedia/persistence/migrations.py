"""
Database migration system for schema updates.
"""

import logging
from typing import List, Tuple
from pathlib import Path
import json
from datetime import datetime

logger = logging.getLogger(__name__)


class MigrationManager:
    """Manages database schema migrations."""
    
    def __init__(self, db_manager):
        """Initialize migration manager.
        
        Args:
            db_manager: DatabaseManager instance
        """
        self.db = db_manager
        self.migrations_table = "schema_migrations"
        
    async def initialize(self):
        """Create migrations tracking table."""
        schema = """
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL,
            checksum TEXT
        );
        """
        await self.db._execute_script(schema)
    
    async def get_current_version(self) -> int:
        """Get current schema version.
        
        Returns:
            Current version number, 0 if no migrations applied
        """
        await self.initialize()
        
        query = "SELECT MAX(version) as version FROM schema_migrations"
        rows = await self.db.execute(query)
        
        if rows and rows[0]['version'] is not None:
            return rows[0]['version']
        return 0
    
    async def apply_migration(self, version: int, name: str, sql: str):
        """Apply a single migration.
        
        Args:
            version: Migration version number
            name: Migration name
            sql: SQL statements to execute
        """
        current = await self.get_current_version()
        
        if version <= current:
            logger.debug(f"Migration {version} ({name}) already applied")
            return
        
        logger.info(f"Applying migration {version}: {name}")
        
        try:
            # Execute migration SQL
            await self.db._execute_script(sql)
            
            # Record migration
            query = """
            INSERT INTO schema_migrations (version, name, applied_at)
            VALUES (?, ?, ?)
            """
            params = (version, name, datetime.now().isoformat())
            await self.db.execute_insert(query, params)
            
            logger.info(f"Migration {version} applied successfully")
        except Exception as e:
            logger.error(f"Failed to apply migration {version}: {e}")
            raise
    
    async def apply_all_migrations(self):
        """Apply all pending migrations."""
        migrations = self.get_migrations()
        current = await self.get_current_version()
        
        for version, name, sql in migrations:
            if version > current:
                await self.apply_migration(version, name, sql)
    
    def get_migrations(self) -> List[Tuple[int, str, str]]:
        """Get all migrations in order.
        
        Returns:
            List of (version, name, sql) tuples
        """
        return [
            (1, "add_pipeline_execution_stats", """
                ALTER TABLE pipelines ADD COLUMN execution_count INTEGER DEFAULT 0;
                ALTER TABLE pipelines ADD COLUMN last_executed_at TEXT;
                ALTER TABLE pipelines ADD COLUMN total_execution_time_ms REAL DEFAULT 0.0;
                ALTER TABLE pipelines ADD COLUMN error_count INTEGER DEFAULT 0;
            """),
            
            (2, "add_node_usage_tracking", """
                ALTER TABLE nodes ADD COLUMN usage_count INTEGER DEFAULT 0;
                ALTER TABLE nodes ADD COLUMN last_used_at TEXT;
                
                CREATE INDEX IF NOT EXISTS idx_nodes_usage ON nodes(usage_count DESC);
                CREATE INDEX IF NOT EXISTS idx_pipelines_execution ON pipelines(execution_count DESC);
            """),
            
            (3, "add_favorites_and_ratings", """
                CREATE TABLE IF NOT EXISTS user_favorites (
                    user_id TEXT NOT NULL,
                    resource_type TEXT NOT NULL,  -- 'pipeline' or 'node'
                    resource_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (user_id, resource_type, resource_id),
                    FOREIGN KEY (user_id) REFERENCES users(id)
                );
                
                CREATE TABLE IF NOT EXISTS user_ratings (
                    user_id TEXT NOT NULL,
                    resource_type TEXT NOT NULL,
                    resource_id TEXT NOT NULL,
                    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
                    comment TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (user_id, resource_type, resource_id),
                    FOREIGN KEY (user_id) REFERENCES users(id)
                );
                
                CREATE INDEX IF NOT EXISTS idx_favorites_user ON user_favorites(user_id);
                CREATE INDEX IF NOT EXISTS idx_ratings_resource ON user_ratings(resource_type, resource_id);
            """),
            
            (4, "add_pipeline_categories", """
                CREATE TABLE IF NOT EXISTS pipeline_categories (
                    id TEXT PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    description TEXT,
                    parent_id TEXT,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (parent_id) REFERENCES pipeline_categories(id)
                );
                
                -- Default categories
                INSERT OR IGNORE INTO pipeline_categories (id, name, description, created_at)
                VALUES 
                    ('audio', 'Audio Processing', 'Audio and speech processing pipelines', datetime('now')),
                    ('video', 'Video Processing', 'Video processing and analysis pipelines', datetime('now')),
                    ('ml', 'Machine Learning', 'ML inference and training pipelines', datetime('now')),
                    ('data', 'Data Processing', 'Data transformation and analysis pipelines', datetime('now')),
                    ('utility', 'Utilities', 'General utility pipelines', datetime('now'));
            """),
            
            (5, "add_pipeline_dependencies", """
                CREATE TABLE IF NOT EXISTS pipeline_dependencies (
                    pipeline_id TEXT NOT NULL,
                    dependency_type TEXT NOT NULL,  -- 'package', 'model', 'service'
                    dependency_name TEXT NOT NULL,
                    dependency_version TEXT,
                    is_required INTEGER DEFAULT 1,
                    PRIMARY KEY (pipeline_id, dependency_type, dependency_name),
                    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE
                );
                
                CREATE INDEX IF NOT EXISTS idx_dependencies_pipeline ON pipeline_dependencies(pipeline_id);
            """),
            
            (6, "add_sharing_tokens", """
                CREATE TABLE IF NOT EXISTS sharing_tokens (
                    token TEXT PRIMARY KEY,
                    resource_type TEXT NOT NULL,
                    resource_id TEXT NOT NULL,
                    created_by TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    max_uses INTEGER,
                    use_count INTEGER DEFAULT 0,
                    permissions TEXT,  -- JSON array of allowed operations
                    FOREIGN KEY (created_by) REFERENCES users(id)
                );
                
                CREATE INDEX IF NOT EXISTS idx_sharing_tokens_resource ON sharing_tokens(resource_type, resource_id);
                CREATE INDEX IF NOT EXISTS idx_sharing_tokens_expiry ON sharing_tokens(expires_at);
            """)
        ]
    
    async def rollback_to_version(self, target_version: int):
        """Rollback to a specific version.
        
        Args:
            target_version: Version to rollback to
        """
        # This would require storing rollback SQL for each migration
        # For now, just log a warning
        logger.warning(f"Rollback to version {target_version} requested but not implemented")
        raise NotImplementedError("Migration rollback not yet implemented")