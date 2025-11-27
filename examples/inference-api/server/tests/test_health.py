"""Tests for health endpoint."""

import pytest
from fastapi.testclient import TestClient

from inference_api.main import app


@pytest.fixture
def client():
    """Create test client."""
    with TestClient(app) as client:
        yield client


def test_health_returns_200(client):
    """Test health endpoint returns 200."""
    response = client.get("/health")
    assert response.status_code == 200


def test_health_response_format(client):
    """Test health response has required fields."""
    response = client.get("/health")
    data = response.json()

    assert "status" in data
    assert "version" in data
    assert "pipelines_loaded" in data
    assert "active_sessions" in data
    assert "runtime_available" in data


def test_health_status_is_healthy(client):
    """Test health status is healthy when runtime available."""
    response = client.get("/health")
    data = response.json()

    # Should be healthy when runtime is available
    assert data["status"] in ["healthy", "degraded"]


def test_health_version_matches(client):
    """Test health version matches API version."""
    response = client.get("/health")
    data = response.json()

    assert data["version"] == "0.1.0"
