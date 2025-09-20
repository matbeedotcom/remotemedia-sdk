"""
Database manager for persistence layer using SQLite.
"""

import sqlite3
import json
import asyncio
from pathlib import Path
from typing import Dict, Any, List, Optional, Tuple
from datetime import datetime
import logging
from contextlib import asynccontextmanager

logger = logging.getLogger(__name__)


class DatabaseManager:
    """Manages SQLite database connection and operations."""
    
    def __init__(self, db_path: str = "pipelines.db"):
        """Initialize database manager.
        
        Args:
            db_path: Path to SQLite database file
        """
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = asyncio.Lock()
        self._initialized = False
        
    async def initialize(self):
        """Initialize database and create tables if needed."""
        async with self._lock:
            if self._initialized:
                return
                
            await self._create_tables()
            self._initialized = True
            logger.info(f"Database initialized at {self.db_path}")
    
    async def _create_tables(self):
        """Create database tables."""
        schema = """
        -- Users table for ownership and access control
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT UNIQUE NOT NULL,
            email TEXT,
            team_id TEXT,
            created_at TEXT NOT NULL,
            is_active INTEGER DEFAULT 1
        );
        
        -- Stored nodes table
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            node_type TEXT NOT NULL,
            config TEXT NOT NULL,  -- JSON
            owner_id TEXT NOT NULL,
            access_level TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            version INTEGER DEFAULT 1,
            description TEXT,
            tags TEXT,  -- JSON array
            metadata TEXT,  -- JSON
            is_template INTEGER DEFAULT 0,
            parent_id TEXT,
            FOREIGN KEY (owner_id) REFERENCES users(id),
            FOREIGN KEY (parent_id) REFERENCES nodes(id)
        );
        
        -- Node versions table
        CREATE TABLE IF NOT EXISTS node_versions (
            id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            config TEXT NOT NULL,  -- JSON
            created_at TEXT NOT NULL,
            created_by TEXT NOT NULL,
            change_description TEXT,
            is_current INTEGER DEFAULT 0,
            FOREIGN KEY (node_id) REFERENCES nodes(id),
            FOREIGN KEY (created_by) REFERENCES users(id),
            UNIQUE(node_id, version)
        );
        
        -- Stored pipelines table
        CREATE TABLE IF NOT EXISTS pipelines (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            definition TEXT NOT NULL,  -- JSON
            owner_id TEXT NOT NULL,
            access_level TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            version INTEGER DEFAULT 1,
            description TEXT,
            tags TEXT,  -- JSON array
            metadata TEXT,  -- JSON
            is_template INTEGER DEFAULT 0,
            parent_id TEXT,
            FOREIGN KEY (owner_id) REFERENCES users(id),
            FOREIGN KEY (parent_id) REFERENCES pipelines(id)
        );
        
        -- Pipeline versions table
        CREATE TABLE IF NOT EXISTS pipeline_versions (
            id TEXT PRIMARY KEY,
            pipeline_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            definition TEXT NOT NULL,  -- JSON
            created_at TEXT NOT NULL,
            created_by TEXT NOT NULL,
            change_description TEXT,
            is_current INTEGER DEFAULT 0,
            FOREIGN KEY (pipeline_id) REFERENCES pipelines(id),
            FOREIGN KEY (created_by) REFERENCES users(id),
            UNIQUE(pipeline_id, version)
        );
        
        -- Pipeline-Node associations
        CREATE TABLE IF NOT EXISTS pipeline_nodes (
            pipeline_id TEXT NOT NULL,
            node_id TEXT NOT NULL,
            position INTEGER,
            PRIMARY KEY (pipeline_id, node_id),
            FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES nodes(id)
        );
        
        -- Access logs for auditing
        CREATE TABLE IF NOT EXISTS access_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            resource_type TEXT NOT NULL,  -- 'pipeline' or 'node'
            resource_id TEXT NOT NULL,
            action TEXT NOT NULL,  -- 'read', 'write', 'execute', 'delete'
            timestamp TEXT NOT NULL,
            metadata TEXT,  -- JSON
            FOREIGN KEY (user_id) REFERENCES users(id)
        );
        
        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_nodes_owner ON nodes(owner_id);
        CREATE INDEX IF NOT EXISTS idx_nodes_access ON nodes(access_level);
        CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
        CREATE INDEX IF NOT EXISTS idx_pipelines_owner ON pipelines(owner_id);
        CREATE INDEX IF NOT EXISTS idx_pipelines_access ON pipelines(access_level);
        CREATE INDEX IF NOT EXISTS idx_access_logs_user ON access_logs(user_id);
        CREATE INDEX IF NOT EXISTS idx_access_logs_resource ON access_logs(resource_type, resource_id);
        """
        
        await self._execute_script(schema)
    
    async def _execute_script(self, script: str):
        """Execute SQL script."""
        def run():
            conn = sqlite3.connect(str(self.db_path))
            try:
                conn.executescript(script)
                conn.commit()
            finally:
                conn.close()
        
        await asyncio.get_event_loop().run_in_executor(None, run)
    
    async def execute(self, query: str, params: Optional[Tuple] = None) -> List[sqlite3.Row]:
        """Execute a query and return results."""
        def run():
            conn = sqlite3.connect(str(self.db_path))
            conn.row_factory = sqlite3.Row
            try:
                cursor = conn.cursor()
                if params:
                    cursor.execute(query, params)
                else:
                    cursor.execute(query)
                return cursor.fetchall()
            finally:
                conn.close()
        
        async with self._lock:
            return await asyncio.get_event_loop().run_in_executor(None, run)
    
    async def execute_many(self, query: str, params_list: List[Tuple]) -> None:
        """Execute a query with multiple parameter sets."""
        def run():
            conn = sqlite3.connect(str(self.db_path))
            try:
                cursor = conn.cursor()
                cursor.executemany(query, params_list)
                conn.commit()
            finally:
                conn.close()
        
        async with self._lock:
            await asyncio.get_event_loop().run_in_executor(None, run)
    
    async def execute_insert(self, query: str, params: Optional[Tuple] = None) -> int:
        """Execute an insert query and return the last row id."""
        def run():
            conn = sqlite3.connect(str(self.db_path))
            try:
                cursor = conn.cursor()
                if params:
                    cursor.execute(query, params)
                else:
                    cursor.execute(query)
                conn.commit()
                return cursor.lastrowid
            finally:
                conn.close()
        
        async with self._lock:
            return await asyncio.get_event_loop().run_in_executor(None, run)
    
    async def execute_update(self, query: str, params: Optional[Tuple] = None) -> int:
        """Execute an update/delete query and return affected rows."""
        def run():
            conn = sqlite3.connect(str(self.db_path))
            try:
                cursor = conn.cursor()
                if params:
                    cursor.execute(query, params)
                else:
                    cursor.execute(query)
                conn.commit()
                return cursor.rowcount
            finally:
                conn.close()
        
        async with self._lock:
            return await asyncio.get_event_loop().run_in_executor(None, run)
    
    @asynccontextmanager
    async def transaction(self):
        """Context manager for database transactions."""
        conn = None
        try:
            conn = sqlite3.connect(str(self.db_path))
            conn.row_factory = sqlite3.Row
            conn.execute("BEGIN")
            yield conn
            conn.commit()
        except Exception:
            if conn:
                conn.rollback()
            raise
        finally:
            if conn:
                conn.close()
    
    async def log_access(self, user_id: str, resource_type: str, 
                         resource_id: str, action: str, 
                         metadata: Optional[Dict[str, Any]] = None):
        """Log an access event for auditing."""
        query = """
        INSERT INTO access_logs (user_id, resource_type, resource_id, action, timestamp, metadata)
        VALUES (?, ?, ?, ?, ?, ?)
        """
        params = (
            user_id,
            resource_type,
            resource_id,
            action,
            datetime.now().isoformat(),
            json.dumps(metadata) if metadata else None
        )
        await self.execute_insert(query, params)
    
    async def close(self):
        """Close database connection."""
        # SQLite connections are closed after each operation
        pass
    
    async def get_user(self, user_id: str) -> Optional[Dict[str, Any]]:
        """Get user by ID."""
        query = "SELECT * FROM users WHERE id = ?"
        rows = await self.execute(query, (user_id,))
        if rows:
            return dict(rows[0])
        return None
    
    async def create_user(self, user_id: str, username: str, 
                         email: Optional[str] = None, 
                         team_id: Optional[str] = None) -> None:
        """Create a new user."""
        query = """
        INSERT OR IGNORE INTO users (id, username, email, team_id, created_at)
        VALUES (?, ?, ?, ?, ?)
        """
        params = (user_id, username, email, team_id, datetime.now().isoformat())
        await self.execute_insert(query, params)