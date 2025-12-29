"""
Unit tests for RuntimeData.File type (Spec 001).

Tests the Python client's RuntimeData.file() factory method and
FileMetadata dataclass for file reference handling.
"""

import pytest
import time

from remotemedia.core.multiprocessing.data import (
    RuntimeData,
    DataType,
    FileMetadata,
    AudioMetadata,
)


class TestFileMetadata:
    """Tests for FileMetadata dataclass."""

    def test_file_metadata_minimal(self):
        """Test FileMetadata with only required path field."""
        meta = FileMetadata(path="/tmp/test.txt")

        assert meta.path == "/tmp/test.txt"
        assert meta.filename is None
        assert meta.mime_type is None
        assert meta.size is None
        assert meta.offset is None
        assert meta.length is None
        assert meta.stream_id is None

    def test_file_metadata_all_fields(self):
        """Test FileMetadata with all fields populated."""
        meta = FileMetadata(
            path="/data/input/video.mp4",
            filename="video.mp4",
            mime_type="video/mp4",
            size=104_857_600,
            offset=1_048_576,
            length=65_536,
            stream_id="video_track",
        )

        assert meta.path == "/data/input/video.mp4"
        assert meta.filename == "video.mp4"
        assert meta.mime_type == "video/mp4"
        assert meta.size == 104_857_600
        assert meta.offset == 1_048_576
        assert meta.length == 65_536
        assert meta.stream_id == "video_track"

    def test_file_metadata_from_ipc(self):
        """Test FileMetadata.from_ipc() classmethod."""
        meta = FileMetadata.from_ipc(
            path="/test/path.bin",
            filename="path.bin",
            mime_type="application/octet-stream",
            size=1024,
            offset=0,
            length=512,
            stream_id="main",
        )

        assert meta.path == "/test/path.bin"
        assert meta.filename == "path.bin"
        assert meta.mime_type == "application/octet-stream"
        assert meta.size == 1024


class TestRuntimeDataFile:
    """Tests for RuntimeData.file() factory method."""

    def test_file_factory_minimal(self):
        """Test RuntimeData.file() with only path."""
        data = RuntimeData.file("/tmp/output.bin")

        assert data.type == DataType.FILE
        assert data.is_file()
        assert data.get_file_path() == "/tmp/output.bin"
        assert isinstance(data.metadata, FileMetadata)
        assert data.metadata.path == "/tmp/output.bin"
        assert data.metadata.filename is None

    def test_file_factory_with_all_fields(self):
        """Test RuntimeData.file() with all optional fields."""
        data = RuntimeData.file(
            path="/data/input/video.mp4",
            session_id="test_session",
            filename="video.mp4",
            mime_type="video/mp4",
            size=104_857_600,
            offset=1_048_576,
            length=65_536,
            stream_id="video_track",
        )

        assert data.type == DataType.FILE
        assert data.session_id == "test_session"
        assert data.is_file()
        assert data.get_file_path() == "/data/input/video.mp4"

        meta = data.metadata
        assert isinstance(meta, FileMetadata)
        assert meta.path == "/data/input/video.mp4"
        assert meta.filename == "video.mp4"
        assert meta.mime_type == "video/mp4"
        assert meta.size == 104_857_600
        assert meta.offset == 1_048_576
        assert meta.length == 65_536
        assert meta.stream_id == "video_track"

    def test_file_factory_byte_range(self):
        """Test RuntimeData.file() for byte range requests."""
        data = RuntimeData.file(
            path="/data/large_file.bin",
            size=1_073_741_824,  # 1 GB
            offset=10 * 1024 * 1024,  # 10 MB offset
            length=64 * 1024,  # 64 KB chunk
        )

        assert data.is_file()
        meta = data.metadata
        assert meta.size == 1_073_741_824
        assert meta.offset == 10 * 1024 * 1024
        assert meta.length == 64 * 1024

    def test_file_data_type_string(self):
        """Test data_type() returns 'file' string."""
        data = RuntimeData.file("/tmp/test.txt")
        assert data.data_type() == "file"

    def test_is_file_true(self):
        """Test is_file() returns True for file data."""
        data = RuntimeData.file("/tmp/test.txt")
        assert data.is_file() is True

    def test_is_file_false_for_other_types(self):
        """Test is_file() returns False for non-file data."""
        import numpy as np

        audio_data = RuntimeData(
            type=DataType.AUDIO,
            payload=np.zeros(100, dtype=np.float32),
            session_id="test",
            timestamp=time.time(),
            metadata=AudioMetadata(sample_rate=48000, channels=1, format=1, duration_ms=100),
        )
        assert audio_data.is_file() is False

    def test_get_file_path(self):
        """Test get_file_path() extracts path correctly."""
        data = RuntimeData.file("/custom/path/to/file.mp4")
        assert data.get_file_path() == "/custom/path/to/file.mp4"

    def test_get_file_path_raises_for_non_file(self):
        """Test get_file_path() raises ValueError for non-file data."""
        import numpy as np

        audio_data = RuntimeData(
            type=DataType.AUDIO,
            payload=np.zeros(100, dtype=np.float32),
            session_id="test",
            timestamp=time.time(),
            metadata=AudioMetadata(sample_rate=48000, channels=1, format=1, duration_ms=100),
        )

        with pytest.raises(ValueError, match="Can only get path from FILE data"):
            audio_data.get_file_path()

    def test_file_payload_is_dict(self):
        """Test that file payload is a dict with all fields."""
        data = RuntimeData.file(
            path="/test/path.txt",
            filename="path.txt",
            mime_type="text/plain",
            size=1024,
        )

        assert isinstance(data.payload, dict)
        assert data.payload["path"] == "/test/path.txt"
        assert data.payload["filename"] == "path.txt"
        assert data.payload["mime_type"] == "text/plain"
        assert data.payload["size"] == 1024

    def test_file_timestamp_auto_set(self):
        """Test that timestamp is auto-set if not provided."""
        before = time.time()
        data = RuntimeData.file("/tmp/test.txt")
        after = time.time()

        assert before <= data.timestamp <= after

    def test_file_with_empty_session_id(self):
        """Test file with default empty session_id."""
        data = RuntimeData.file("/tmp/test.txt")
        assert data.session_id == ""


class TestRuntimeDataFileValidation:
    """Tests for RuntimeData File validation in __post_init__."""

    def test_file_from_dict_payload(self):
        """Test creating File from dict payload auto-creates metadata."""
        data = RuntimeData(
            type=DataType.FILE,
            payload={
                "path": "/test/path.bin",
                "filename": "path.bin",
                "mime_type": "application/octet-stream",
                "size": 2048,
            },
            session_id="test",
            timestamp=time.time(),
        )

        assert data.is_file()
        assert isinstance(data.metadata, FileMetadata)
        assert data.metadata.path == "/test/path.bin"
        assert data.metadata.filename == "path.bin"
        assert data.metadata.size == 2048

    def test_file_requires_path_in_dict(self):
        """Test that File with dict payload requires path field."""
        # Dict with path should work
        data = RuntimeData(
            type=DataType.FILE,
            payload={"path": "/valid/path.txt"},
            session_id="test",
            timestamp=time.time(),
        )
        assert data.metadata.path == "/valid/path.txt"

    def test_file_invalid_payload_raises(self):
        """Test that File with non-dict, non-metadata payload raises."""
        with pytest.raises(ValueError, match="File requires FileMetadata or dict payload"):
            RuntimeData(
                type=DataType.FILE,
                payload="invalid_string_payload",
                session_id="test",
                timestamp=time.time(),
            )


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
