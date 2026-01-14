"""
Node storage and retrieval operations.
"""

import json
import uuid
from datetime import datetime
from typing import Dict, Any, List, Optional
from .database import DatabaseManager
from .models import StoredNode, NodeVersion, AccessLevel
import logging

logger = logging.getLogger(__name__)


class NodeStore:
    """Manages node persistence operations."""
    
    def __init__(self, db_manager: DatabaseManager):
        """Initialize node store.
        
        Args:
            db_manager: Database manager instance
        """
        self.db = db_manager
    
    async def create_node(self, 
                         name: str,
                         node_type: str,
                         config: Dict[str, Any],
                         owner_id: str,
                         access_level: AccessLevel = AccessLevel.PRIVATE,
                         description: Optional[str] = None,
                         tags: Optional[List[str]] = None,
                         metadata: Optional[Dict[str, Any]] = None,
                         is_template: bool = False) -> StoredNode:
        """Create and store a new node.
        
        Args:
            name: Node name
            node_type: Type of node (e.g., 'CalculatorNode')
            config: Node configuration
            owner_id: User ID of the owner
            access_level: Access control level
            description: Optional description
            tags: Optional list of tags
            metadata: Optional metadata
            is_template: Whether this is a template node
            
        Returns:
            Created StoredNode instance
        """
        node_id = str(uuid.uuid4())
        now = datetime.now()
        
        stored_node = StoredNode(
            id=node_id,
            name=name,
            node_type=node_type,
            config=config,
            owner_id=owner_id,
            access_level=access_level,
            created_at=now,
            updated_at=now,
            version=1,
            description=description,
            tags=tags or [],
            metadata=metadata or {},
            is_template=is_template
        )
        
        query = """
        INSERT INTO nodes (
            id, name, node_type, config, owner_id, access_level,
            created_at, updated_at, version, description, tags, 
            metadata, is_template, parent_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """
        
        params = (
            stored_node.id,
            stored_node.name,
            stored_node.node_type,
            json.dumps(stored_node.config),
            stored_node.owner_id,
            stored_node.access_level.value,
            stored_node.created_at.isoformat(),
            stored_node.updated_at.isoformat(),
            stored_node.version,
            stored_node.description,
            json.dumps(stored_node.tags),
            json.dumps(stored_node.metadata),
            1 if stored_node.is_template else 0,
            stored_node.parent_id
        )
        
        await self.db.execute_insert(query, params)
        
        # Create initial version record
        await self._create_version(stored_node, owner_id)
        
        # Log the creation
        await self.db.log_access(owner_id, 'node', node_id, 'create')
        
        logger.info(f"Created node {node_id} ({name}) by user {owner_id}")
        return stored_node
    
    async def get_node(self, node_id: str, user_id: Optional[str] = None) -> Optional[StoredNode]:
        """Get a node by ID.
        
        Args:
            node_id: Node ID
            user_id: Optional user ID for access control
            
        Returns:
            StoredNode if found and accessible, None otherwise
        """
        query = "SELECT * FROM nodes WHERE id = ?"
        rows = await self.db.execute(query, (node_id,))
        
        if not rows:
            return None
        
        row = dict(rows[0])
        
        # Check access control
        if not await self._can_access(row, user_id, 'read'):
            logger.warning(f"User {user_id} denied access to node {node_id}")
            return None
        
        # Parse JSON fields
        row['config'] = json.loads(row['config'])
        row['tags'] = json.loads(row['tags']) if row['tags'] else []
        row['metadata'] = json.loads(row['metadata']) if row['metadata'] else {}
        row['access_level'] = AccessLevel(row['access_level'])
        row['created_at'] = datetime.fromisoformat(row['created_at'])
        row['updated_at'] = datetime.fromisoformat(row['updated_at'])
        row['is_template'] = bool(row['is_template'])
        
        # Log access
        if user_id:
            await self.db.log_access(user_id, 'node', node_id, 'read')
        
        return StoredNode(**row)
    
    async def update_node(self, 
                         node_id: str,
                         user_id: str,
                         config: Optional[Dict[str, Any]] = None,
                         name: Optional[str] = None,
                         description: Optional[str] = None,
                         tags: Optional[List[str]] = None,
                         metadata: Optional[Dict[str, Any]] = None,
                         access_level: Optional[AccessLevel] = None) -> Optional[StoredNode]:
        """Update an existing node.
        
        Args:
            node_id: Node ID
            user_id: User performing the update
            config: Optional new configuration
            name: Optional new name
            description: Optional new description
            tags: Optional new tags
            metadata: Optional new metadata
            access_level: Optional new access level
            
        Returns:
            Updated StoredNode if successful, None otherwise
        """
        # Get existing node
        existing = await self.get_node(node_id)
        if not existing:
            return None
        
        # Check write access
        if not await self._can_access(existing.to_dict(), user_id, 'write'):
            logger.warning(f"User {user_id} denied write access to node {node_id}")
            return None
        
        # Update fields
        if config is not None:
            existing.config = config
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
        UPDATE nodes SET
            name = ?, config = ?, description = ?, tags = ?, 
            metadata = ?, access_level = ?, updated_at = ?, version = ?
        WHERE id = ?
        """
        
        params = (
            existing.name,
            json.dumps(existing.config),
            existing.description,
            json.dumps(existing.tags),
            json.dumps(existing.metadata),
            existing.access_level.value,
            existing.updated_at.isoformat(),
            existing.version,
            node_id
        )
        
        await self.db.execute_update(query, params)
        
        # Create version record
        await self._create_version(existing, user_id)
        
        # Log the update
        await self.db.log_access(user_id, 'node', node_id, 'write')
        
        logger.info(f"Updated node {node_id} by user {user_id}")
        return existing
    
    async def delete_node(self, node_id: str, user_id: str) -> bool:
        """Delete a node.
        
        Args:
            node_id: Node ID
            user_id: User performing the deletion
            
        Returns:
            True if deleted, False otherwise
        """
        # Get existing node
        existing = await self.get_node(node_id)
        if not existing:
            return False
        
        # Check delete access (only owner can delete)
        if existing.owner_id != user_id:
            logger.warning(f"User {user_id} denied delete access to node {node_id}")
            return False
        
        # Delete from database
        query = "DELETE FROM nodes WHERE id = ?"
        affected = await self.db.execute_update(query, (node_id,))
        
        # Log the deletion
        await self.db.log_access(user_id, 'node', node_id, 'delete')
        
        logger.info(f"Deleted node {node_id} by user {user_id}")
        return affected > 0
    
    async def list_nodes(self, 
                        user_id: Optional[str] = None,
                        owner_id: Optional[str] = None,
                        node_type: Optional[str] = None,
                        tags: Optional[List[str]] = None,
                        access_level: Optional[AccessLevel] = None,
                        is_template: Optional[bool] = None,
                        limit: int = 100,
                        offset: int = 0) -> List[StoredNode]:
        """List nodes with filtering.
        
        Args:
            user_id: User requesting the list (for access control)
            owner_id: Filter by owner
            node_type: Filter by node type
            tags: Filter by tags
            access_level: Filter by access level
            is_template: Filter templates only
            limit: Maximum number of results
            offset: Offset for pagination
            
        Returns:
            List of StoredNode instances
        """
        conditions = []
        params = []
        
        if owner_id:
            conditions.append("owner_id = ?")
            params.append(owner_id)
        
        if node_type:
            conditions.append("node_type = ?")
            params.append(node_type)
        
        if access_level:
            conditions.append("access_level = ?")
            params.append(access_level.value)
        
        if is_template is not None:
            conditions.append("is_template = ?")
            params.append(1 if is_template else 0)
        
        where_clause = f"WHERE {' AND '.join(conditions)}" if conditions else ""
        
        query = f"""
        SELECT * FROM nodes
        {where_clause}
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        """
        
        params.extend([limit, offset])
        rows = await self.db.execute(query, tuple(params))
        
        nodes = []
        for row in rows:
            row_dict = dict(row)
            
            # Check access control
            if not await self._can_access(row_dict, user_id, 'read'):
                continue
            
            # Parse JSON fields
            row_dict['config'] = json.loads(row_dict['config'])
            row_dict['tags'] = json.loads(row_dict['tags']) if row_dict['tags'] else []
            row_dict['metadata'] = json.loads(row_dict['metadata']) if row_dict['metadata'] else {}
            row_dict['access_level'] = AccessLevel(row_dict['access_level'])
            row_dict['created_at'] = datetime.fromisoformat(row_dict['created_at'])
            row_dict['updated_at'] = datetime.fromisoformat(row_dict['updated_at'])
            row_dict['is_template'] = bool(row_dict['is_template'])
            
            nodes.append(StoredNode(**row_dict))
        
        # Filter by tags if specified
        if tags:
            nodes = [n for n in nodes if any(tag in n.tags for tag in tags)]
        
        return nodes
    
    async def clone_node(self, node_id: str, user_id: str, 
                        new_name: Optional[str] = None) -> Optional[StoredNode]:
        """Clone an existing node.
        
        Args:
            node_id: Node to clone
            user_id: User creating the clone
            new_name: Optional name for the clone
            
        Returns:
            Cloned StoredNode if successful
        """
        # Get original node
        original = await self.get_node(node_id, user_id)
        if not original:
            return None
        
        # Create clone with new ID
        clone_name = new_name or f"{original.name} (Copy)"
        clone = await self.create_node(
            name=clone_name,
            node_type=original.node_type,
            config=original.config.copy(),
            owner_id=user_id,
            access_level=AccessLevel.PRIVATE,  # Clones start as private
            description=original.description,
            tags=original.tags.copy(),
            metadata={**original.metadata, 'cloned_from': node_id},
            is_template=original.is_template
        )
        
        logger.info(f"Cloned node {node_id} to {clone.id} by user {user_id}")
        return clone
    
    async def get_node_versions(self, node_id: str, 
                               user_id: Optional[str] = None) -> List[NodeVersion]:
        """Get version history for a node.
        
        Args:
            node_id: Node ID
            user_id: User requesting versions (for access control)
            
        Returns:
            List of NodeVersion instances
        """
        # Check access to the node
        node = await self.get_node(node_id, user_id)
        if not node:
            return []
        
        query = """
        SELECT * FROM node_versions 
        WHERE node_id = ?
        ORDER BY version DESC
        """
        
        rows = await self.db.execute(query, (node_id,))
        
        versions = []
        for row in rows:
            row_dict = dict(row)
            row_dict['config'] = json.loads(row_dict['config'])
            row_dict['created_at'] = datetime.fromisoformat(row_dict['created_at'])
            row_dict['is_current'] = bool(row_dict['is_current'])
            versions.append(NodeVersion(**row_dict))
        
        return versions
    
    async def _create_version(self, node: StoredNode, user_id: str, 
                             change_description: Optional[str] = None):
        """Create a version record for a node."""
        version_id = str(uuid.uuid4())
        
        # Mark previous versions as not current
        await self.db.execute_update(
            "UPDATE node_versions SET is_current = 0 WHERE node_id = ?",
            (node.id,)
        )
        
        # Insert new version
        query = """
        INSERT INTO node_versions (
            id, node_id, version, config, created_at, 
            created_by, change_description, is_current
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """
        
        params = (
            version_id,
            node.id,
            node.version,
            json.dumps(node.config),
            datetime.now().isoformat(),
            user_id,
            change_description,
            1
        )
        
        await self.db.execute_insert(query, params)
    
    async def _can_access(self, node_dict: Dict[str, Any], 
                         user_id: Optional[str], action: str) -> bool:
        """Check if a user can access a node.
        
        Args:
            node_dict: Node dictionary from database
            user_id: User ID (None for anonymous)
            action: Action to check ('read', 'write', 'delete')
            
        Returns:
            True if access allowed
        """
        if not user_id:
            # Anonymous users can only read public nodes
            return (action == 'read' and 
                   node_dict['access_level'] in ['public', 'readonly'])
        
        # Owner has full access
        if node_dict['owner_id'] == user_id:
            return True
        
        access_level = node_dict['access_level']
        
        if action == 'read':
            return access_level in ['public', 'readonly', 'team']
        elif action == 'write':
            # Only owner can write to private nodes
            if access_level == 'private':
                return False
            # Team members can write to team nodes
            if access_level == 'team':
                # TODO: Check team membership
                return False
            # Public nodes allow writes from anyone
            return access_level == 'public'
        else:  # delete
            # Only owner can delete
            return False