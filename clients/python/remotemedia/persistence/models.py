"""
Data models for pipeline and node persistence.
"""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Dict, Any, List, Optional
from enum import Enum
import json


class AccessLevel(Enum):
    """Access control levels for stored resources."""
    PRIVATE = "private"      # Only owner can access
    TEAM = "team"            # Team members can access
    PUBLIC = "public"        # Anyone can access
    READONLY = "readonly"    # Public read, owner write


@dataclass
class StoredNode:
    """Represents a persisted node configuration."""
    id: str
    name: str
    node_type: str
    config: Dict[str, Any]
    owner_id: str
    access_level: AccessLevel
    created_at: datetime
    updated_at: datetime
    version: int = 1
    description: Optional[str] = None
    tags: List[str] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    is_template: bool = False
    parent_id: Optional[str] = None  # For versioning
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            'id': self.id,
            'name': self.name,
            'node_type': self.node_type,
            'config': self.config,
            'owner_id': self.owner_id,
            'access_level': self.access_level.value,
            'created_at': self.created_at.isoformat(),
            'updated_at': self.updated_at.isoformat(),
            'version': self.version,
            'description': self.description,
            'tags': self.tags,
            'metadata': self.metadata,
            'is_template': self.is_template,
            'parent_id': self.parent_id
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'StoredNode':
        """Create from dictionary."""
        data = data.copy()
        data['access_level'] = AccessLevel(data['access_level'])
        data['created_at'] = datetime.fromisoformat(data['created_at'])
        data['updated_at'] = datetime.fromisoformat(data['updated_at'])
        return cls(**data)


@dataclass
class StoredPipeline:
    """Represents a persisted pipeline configuration."""
    id: str
    name: str
    definition: Dict[str, Any]  # Exported pipeline definition
    owner_id: str
    access_level: AccessLevel
    created_at: datetime
    updated_at: datetime
    version: int = 1
    description: Optional[str] = None
    tags: List[str] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    is_template: bool = False
    parent_id: Optional[str] = None  # For versioning
    node_ids: List[str] = field(default_factory=list)  # References to stored nodes
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            'id': self.id,
            'name': self.name,
            'definition': self.definition,
            'owner_id': self.owner_id,
            'access_level': self.access_level.value,
            'created_at': self.created_at.isoformat(),
            'updated_at': self.updated_at.isoformat(),
            'version': self.version,
            'description': self.description,
            'tags': self.tags,
            'metadata': self.metadata,
            'is_template': self.is_template,
            'parent_id': self.parent_id,
            'node_ids': self.node_ids
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'StoredPipeline':
        """Create from dictionary."""
        data = data.copy()
        data['access_level'] = AccessLevel(data['access_level'])
        data['created_at'] = datetime.fromisoformat(data['created_at'])
        data['updated_at'] = datetime.fromisoformat(data['updated_at'])
        return cls(**data)


@dataclass
class PipelineVersion:
    """Represents a version of a pipeline."""
    id: str
    pipeline_id: str
    version: int
    definition: Dict[str, Any]
    created_at: datetime
    created_by: str
    change_description: Optional[str] = None
    is_current: bool = False
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            'id': self.id,
            'pipeline_id': self.pipeline_id,
            'version': self.version,
            'definition': self.definition,
            'created_at': self.created_at.isoformat(),
            'created_by': self.created_by,
            'change_description': self.change_description,
            'is_current': self.is_current
        }


@dataclass
class NodeVersion:
    """Represents a version of a node."""
    id: str
    node_id: str
    version: int
    config: Dict[str, Any]
    created_at: datetime
    created_by: str
    change_description: Optional[str] = None
    is_current: bool = False
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            'id': self.id,
            'node_id': self.node_id,
            'version': self.version,
            'config': self.config,
            'created_at': self.created_at.isoformat(),
            'created_by': self.created_by,
            'change_description': self.change_description,
            'is_current': self.is_current
        }


@dataclass
class User:
    """Represents a user for access control."""
    id: str
    username: str
    email: Optional[str] = None
    team_id: Optional[str] = None
    created_at: datetime = field(default_factory=datetime.now)
    is_active: bool = True
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            'id': self.id,
            'username': self.username,
            'email': self.email,
            'team_id': self.team_id,
            'created_at': self.created_at.isoformat(),
            'is_active': self.is_active
        }