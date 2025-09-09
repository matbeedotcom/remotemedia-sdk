"""
Base serialization classes for the RemoteMedia SDK.
"""

import json
import pickle
from abc import ABC, abstractmethod
from typing import Any

from ..core.exceptions import SerializationError


class Serializer(ABC):
    """Base class for data serializers."""
    
    @abstractmethod
    def serialize(self, data: Any) -> bytes:
        """Serialize data to bytes."""
        pass
    
    @abstractmethod
    def deserialize(self, data: bytes) -> Any:
        """Deserialize bytes to data."""
        pass


class JSONSerializer(Serializer):
    """JSON-based serializer."""
    
    def serialize(self, data: Any) -> bytes:
        """Serialize data to JSON bytes."""
        try:
            return json.dumps(data).encode('utf-8')
        except (TypeError, ValueError) as e:
            raise SerializationError(f"JSON serialization failed: {e}") from e
    
    def deserialize(self, data: bytes) -> Any:
        """Deserialize JSON bytes to data."""
        try:
            return json.loads(data.decode('utf-8'))
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            raise SerializationError(f"JSON deserialization failed: {e}") from e


class PickleSerializer(Serializer):
    """Pickle-based serializer."""
    
    def serialize(self, data: Any) -> bytes:
        """Serialize data to pickle bytes."""
        try:
            return pickle.dumps(data)
        except (pickle.PicklingError, TypeError) as e:
            raise SerializationError(f"Pickle serialization failed: {e}") from e
    
    def deserialize(self, data: bytes) -> Any:
        """Deserialize pickle bytes to data."""
        try:
            return pickle.loads(data)
        except (pickle.UnpicklingError, TypeError) as e:
            raise SerializationError(f"Pickle deserialization failed: {e}") from e 