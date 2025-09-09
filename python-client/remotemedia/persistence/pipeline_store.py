"""
Pipeline storage and retrieval operations.
"""

import json
import uuid
from datetime import datetime
from typing import Dict, Any, List, Optional, Set
from .database import DatabaseManager
from .models import StoredPipeline, PipelineVersion, AccessLevel
from .node_store import NodeStore
import logging

logger = logging.getLogger(__name__)


class PipelineStore:
    """Manages pipeline persistence operations."""
    
    def __init__(self, db_manager: DatabaseManager):
        """Initialize pipeline store.
        
        Args:
            db_manager: Database manager instance
        """
        self.db = db_manager
        self.node_store = NodeStore(db_manager)
    
    async def create_pipeline(self,
                             name: str,
                             definition: Dict[str, Any],
                             owner_id: str,
                             access_level: AccessLevel = AccessLevel.PRIVATE,
                             description: Optional[str] = None,
                             tags: Optional[List[str]] = None,
                             metadata: Optional[Dict[str, Any]] = None,
                             is_template: bool = False,
                             persist_nodes: bool = False) -> StoredPipeline:
        """Create and store a new pipeline.
        
        Args:
            name: Pipeline name
            definition: Pipeline definition (exported format)
            owner_id: User ID of the owner
            access_level: Access control level
            description: Optional description
            tags: Optional list of tags
            metadata: Optional metadata
            is_template: Whether this is a template pipeline
            persist_nodes: Whether to also persist individual nodes
            
        Returns:
            Created StoredPipeline instance
        """
        pipeline_id = str(uuid.uuid4())
        now = datetime.now()
        node_ids = []
        
        # Optionally persist individual nodes
        if persist_nodes and 'nodes' in definition:
            for node_def in definition['nodes']:
                if isinstance(node_def, dict) and 'type' in node_def:
                    stored_node = await self.node_store.create_node(
                        name=node_def.get('name', f"{name}_node_{len(node_ids)}"),
                        node_type=node_def['type'],
                        config=node_def.get('config', {}),
                        owner_id=owner_id,
                        access_level=access_level,
                        metadata={'pipeline_id': pipeline_id}
                    )
                    node_ids.append(stored_node.id)
        
        stored_pipeline = StoredPipeline(
            id=pipeline_id,
            name=name,
            definition=definition,
            owner_id=owner_id,
            access_level=access_level,
            created_at=now,
            updated_at=now,
            version=1,
            description=description,
            tags=tags or [],
            metadata=metadata or {},
            is_template=is_template,
            node_ids=node_ids
        )
        
        query = """
        INSERT INTO pipelines (
            id, name, definition, owner_id, access_level,
            created_at, updated_at, version, description, tags,
            metadata, is_template, parent_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """
        
        params = (
            stored_pipeline.id,
            stored_pipeline.name,
            json.dumps(stored_pipeline.definition),
            stored_pipeline.owner_id,
            stored_pipeline.access_level.value,
            stored_pipeline.created_at.isoformat(),
            stored_pipeline.updated_at.isoformat(),
            stored_pipeline.version,
            stored_pipeline.description,
            json.dumps(stored_pipeline.tags),
            json.dumps(stored_pipeline.metadata),
            1 if stored_pipeline.is_template else 0,
            stored_pipeline.parent_id
        )
        
        await self.db.execute_insert(query, params)
        
        # Create pipeline-node associations
        if node_ids:
            await self._associate_nodes(pipeline_id, node_ids)
        
        # Create initial version record
        await self._create_version(stored_pipeline, owner_id)
        
        # Log the creation
        await self.db.log_access(owner_id, 'pipeline', pipeline_id, 'create')
        
        logger.info(f"Created pipeline {pipeline_id} ({name}) by user {owner_id}")
        return stored_pipeline
    
    async def get_pipeline(self, pipeline_id: str, 
                          user_id: Optional[str] = None) -> Optional[StoredPipeline]:
        """Get a pipeline by ID.
        
        Args:
            pipeline_id: Pipeline ID
            user_id: Optional user ID for access control
            
        Returns:
            StoredPipeline if found and accessible, None otherwise
        """
        query = "SELECT * FROM pipelines WHERE id = ?"
        rows = await self.db.execute(query, (pipeline_id,))
        
        if not rows:
            return None
        
        row = dict(rows[0])
        
        # Check access control
        if not await self._can_access(row, user_id, 'read'):
            logger.warning(f"User {user_id} denied access to pipeline {pipeline_id}")
            return None
        
        # Parse JSON fields
        row['definition'] = json.loads(row['definition'])
        row['tags'] = json.loads(row['tags']) if row['tags'] else []
        row['metadata'] = json.loads(row['metadata']) if row['metadata'] else {}
        row['access_level'] = AccessLevel(row['access_level'])
        row['created_at'] = datetime.fromisoformat(row['created_at'])
        row['updated_at'] = datetime.fromisoformat(row['updated_at'])
        row['is_template'] = bool(row['is_template'])
        
        # Get associated node IDs
        node_query = "SELECT node_id FROM pipeline_nodes WHERE pipeline_id = ? ORDER BY position"
        node_rows = await self.db.execute(node_query, (pipeline_id,))
        row['node_ids'] = [r['node_id'] for r in node_rows]
        
        # Log access
        if user_id:
            await self.db.log_access(user_id, 'pipeline', pipeline_id, 'read')
        
        return StoredPipeline(**row)
    
    async def update_pipeline(self,
                            pipeline_id: str,
                            user_id: str,
                            definition: Optional[Dict[str, Any]] = None,
                            name: Optional[str] = None,
                            description: Optional[str] = None,
                            tags: Optional[List[str]] = None,
                            metadata: Optional[Dict[str, Any]] = None,
                            access_level: Optional[AccessLevel] = None) -> Optional[StoredPipeline]:
        """Update an existing pipeline.
        
        Args:
            pipeline_id: Pipeline ID
            user_id: User performing the update
            definition: Optional new definition
            name: Optional new name
            description: Optional new description
            tags: Optional new tags
            metadata: Optional new metadata
            access_level: Optional new access level
            
        Returns:
            Updated StoredPipeline if successful, None otherwise
        """
        # Get existing pipeline
        existing = await self.get_pipeline(pipeline_id)
        if not existing:
            return None
        
        # Check write access
        if not await self._can_access(existing.to_dict(), user_id, 'write'):
            logger.warning(f"User {user_id} denied write access to pipeline {pipeline_id}")
            return None
        
        # Update fields
        if definition is not None:
            existing.definition = definition
        if name is not None:
            existing.name = name
        if description is not None:
            existing.description = description
        if tags is not None:
            existing.tags = tags
        if metadata is not None:
            existing.metadata = metadata
        if access_level is not None:
            existing.access_level = access_level
        
        existing.updated_at = datetime.now()
        existing.version += 1
        
        # Update database
        query = """
        UPDATE pipelines SET
            name = ?, definition = ?, description = ?, tags = ?,
            metadata = ?, access_level = ?, updated_at = ?, version = ?
        WHERE id = ?
        """
        
        params = (
            existing.name,
            json.dumps(existing.definition),
            existing.description,
            json.dumps(existing.tags),
            json.dumps(existing.metadata),
            existing.access_level.value,
            existing.updated_at.isoformat(),
            existing.version,
            pipeline_id
        )
        
        await self.db.execute_update(query, params)
        
        # Create version record
        await self._create_version(existing, user_id)
        
        # Log the update
        await self.db.log_access(user_id, 'pipeline', pipeline_id, 'write')
        
        logger.info(f"Updated pipeline {pipeline_id} by user {user_id}")
        return existing
    
    async def delete_pipeline(self, pipeline_id: str, user_id: str) -> bool:
        """Delete a pipeline.
        
        Args:
            pipeline_id: Pipeline ID
            user_id: User performing the deletion
            
        Returns:
            True if deleted, False otherwise
        """
        # Get existing pipeline
        existing = await self.get_pipeline(pipeline_id)
        if not existing:
            return False
        
        # Check delete access (only owner can delete)
        if existing.owner_id != user_id:
            logger.warning(f"User {user_id} denied delete access to pipeline {pipeline_id}")
            return False
        
        # Delete from database (cascade will handle associations)
        query = "DELETE FROM pipelines WHERE id = ?"
        affected = await self.db.execute_update(query, (pipeline_id,))
        
        # Log the deletion
        await self.db.log_access(user_id, 'pipeline', pipeline_id, 'delete')
        
        logger.info(f"Deleted pipeline {pipeline_id} by user {user_id}")
        return affected > 0
    
    async def list_pipelines(self,
                           user_id: Optional[str] = None,
                           owner_id: Optional[str] = None,
                           tags: Optional[List[str]] = None,
                           access_level: Optional[AccessLevel] = None,
                           is_template: Optional[bool] = None,
                           search: Optional[str] = None,
                           limit: int = 100,
                           offset: int = 0) -> List[StoredPipeline]:
        """List pipelines with filtering.
        
        Args:
            user_id: User requesting the list (for access control)
            owner_id: Filter by owner
            tags: Filter by tags
            access_level: Filter by access level
            is_template: Filter templates only
            search: Search in name and description
            limit: Maximum number of results
            offset: Offset for pagination
            
        Returns:
            List of StoredPipeline instances
        """
        conditions = []
        params = []
        
        if owner_id:
            conditions.append("owner_id = ?")
            params.append(owner_id)
        
        if access_level:
            conditions.append("access_level = ?")
            params.append(access_level.value)
        
        if is_template is not None:
            conditions.append("is_template = ?")
            params.append(1 if is_template else 0)
        
        if search:
            conditions.append("(name LIKE ? OR description LIKE ?)")
            search_pattern = f"%{search}%"
            params.extend([search_pattern, search_pattern])
        
        where_clause = f"WHERE {' AND '.join(conditions)}" if conditions else ""
        
        query = f"""
        SELECT * FROM pipelines
        {where_clause}
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        """
        
        params.extend([limit, offset])
        rows = await self.db.execute(query, tuple(params))
        
        pipelines = []
        for row in rows:
            row_dict = dict(row)
            
            # Check access control
            if not await self._can_access(row_dict, user_id, 'read'):
                continue
            
            # Parse JSON fields
            row_dict['definition'] = json.loads(row_dict['definition'])
            row_dict['tags'] = json.loads(row_dict['tags']) if row_dict['tags'] else []
            row_dict['metadata'] = json.loads(row_dict['metadata']) if row_dict['metadata'] else {}
            row_dict['access_level'] = AccessLevel(row_dict['access_level'])
            row_dict['created_at'] = datetime.fromisoformat(row_dict['created_at'])
            row_dict['updated_at'] = datetime.fromisoformat(row_dict['updated_at'])
            row_dict['is_template'] = bool(row_dict['is_template'])
            
            # Get node IDs
            node_query = "SELECT node_id FROM pipeline_nodes WHERE pipeline_id = ?"
            node_rows = await self.db.execute(node_query, (row_dict['id'],))
            row_dict['node_ids'] = [r['node_id'] for r in node_rows]
            
            pipelines.append(StoredPipeline(**row_dict))
        
        # Filter by tags if specified
        if tags:
            pipelines = [p for p in pipelines if any(tag in p.tags for tag in tags)]
        
        return pipelines
    
    async def clone_pipeline(self, pipeline_id: str, user_id: str,
                           new_name: Optional[str] = None,
                           clone_nodes: bool = False) -> Optional[StoredPipeline]:
        """Clone an existing pipeline.
        
        Args:
            pipeline_id: Pipeline to clone
            user_id: User creating the clone
            new_name: Optional name for the clone
            clone_nodes: Whether to also clone associated nodes
            
        Returns:
            Cloned StoredPipeline if successful
        """
        # Get original pipeline
        original = await self.get_pipeline(pipeline_id, user_id)
        if not original:
            return None
        
        # Clone associated nodes if requested
        node_ids = []
        if clone_nodes and original.node_ids:
            for node_id in original.node_ids:
                cloned_node = await self.node_store.clone_node(node_id, user_id)
                if cloned_node:
                    node_ids.append(cloned_node.id)
        
        # Create clone with new ID
        clone_name = new_name or f"{original.name} (Copy)"
        clone = await self.create_pipeline(
            name=clone_name,
            definition=original.definition.copy(),
            owner_id=user_id,
            access_level=AccessLevel.PRIVATE,  # Clones start as private
            description=original.description,
            tags=original.tags.copy(),
            metadata={**original.metadata, 'cloned_from': pipeline_id},
            is_template=original.is_template
        )
        
        # Associate cloned nodes
        if node_ids:
            await self._associate_nodes(clone.id, node_ids)
            clone.node_ids = node_ids
        
        logger.info(f"Cloned pipeline {pipeline_id} to {clone.id} by user {user_id}")
        return clone
    
    async def get_pipeline_versions(self, pipeline_id: str,
                                   user_id: Optional[str] = None) -> List[PipelineVersion]:
        """Get version history for a pipeline.
        
        Args:
            pipeline_id: Pipeline ID
            user_id: User requesting versions (for access control)
            
        Returns:
            List of PipelineVersion instances
        """
        # Check access to the pipeline
        pipeline = await self.get_pipeline(pipeline_id, user_id)
        if not pipeline:
            return []
        
        query = """
        SELECT * FROM pipeline_versions
        WHERE pipeline_id = ?
        ORDER BY version DESC
        """
        
        rows = await self.db.execute(query, (pipeline_id,))
        
        versions = []
        for row in rows:
            row_dict = dict(row)
            row_dict['definition'] = json.loads(row_dict['definition'])
            row_dict['created_at'] = datetime.fromisoformat(row_dict['created_at'])
            row_dict['is_current'] = bool(row_dict['is_current'])
            versions.append(PipelineVersion(**row_dict))
        
        return versions
    
    async def get_templates(self, user_id: Optional[str] = None) -> List[StoredPipeline]:
        """Get available pipeline templates.
        
        Args:
            user_id: User requesting templates
            
        Returns:
            List of template pipelines
        """
        return await self.list_pipelines(
            user_id=user_id,
            is_template=True,
            access_level=AccessLevel.PUBLIC
        )
    
    async def _associate_nodes(self, pipeline_id: str, node_ids: List[str]):
        """Create pipeline-node associations."""
        # Delete existing associations
        await self.db.execute_update(
            "DELETE FROM pipeline_nodes WHERE pipeline_id = ?",
            (pipeline_id,)
        )
        
        # Insert new associations
        if node_ids:
            query = "INSERT INTO pipeline_nodes (pipeline_id, node_id, position) VALUES (?, ?, ?)"
            params_list = [(pipeline_id, node_id, i) for i, node_id in enumerate(node_ids)]
            await self.db.execute_many(query, params_list)
    
    async def _create_version(self, pipeline: StoredPipeline, user_id: str,
                             change_description: Optional[str] = None):
        """Create a version record for a pipeline."""
        version_id = str(uuid.uuid4())
        
        # Mark previous versions as not current
        await self.db.execute_update(
            "UPDATE pipeline_versions SET is_current = 0 WHERE pipeline_id = ?",
            (pipeline.id,)
        )
        
        # Insert new version
        query = """
        INSERT INTO pipeline_versions (
            id, pipeline_id, version, definition, created_at,
            created_by, change_description, is_current
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """
        
        params = (
            version_id,
            pipeline.id,
            pipeline.version,
            json.dumps(pipeline.definition),
            datetime.now().isoformat(),
            user_id,
            change_description,
            1
        )
        
        await self.db.execute_insert(query, params)
    
    async def _can_access(self, pipeline_dict: Dict[str, Any],
                         user_id: Optional[str], action: str) -> bool:
        """Check if a user can access a pipeline.
        
        Args:
            pipeline_dict: Pipeline dictionary from database
            user_id: User ID (None for anonymous)
            action: Action to check ('read', 'write', 'delete', 'execute')
            
        Returns:
            True if access allowed
        """
        if not user_id:
            # Anonymous users can only read/execute public pipelines
            return (action in ['read', 'execute'] and
                   pipeline_dict['access_level'] in ['public', 'readonly'])
        
        # Owner has full access
        if pipeline_dict['owner_id'] == user_id:
            return True
        
        access_level = pipeline_dict['access_level']
        
        if action in ['read', 'execute']:
            return access_level in ['public', 'readonly', 'team']
        elif action == 'write':
            # Only owner can write to private pipelines
            if access_level == 'private':
                return False
            # Team members can write to team pipelines
            if access_level == 'team':
                # TODO: Check team membership
                return False
            # Public pipelines allow writes from anyone
            return access_level == 'public'
        else:  # delete
            # Only owner can delete
            return False