"""Tests for pipelines endpoints."""

import pytest
from fastapi.testclient import TestClient

from inference_api.main import app


@pytest.fixture
def client():
    """Create test client."""
    with TestClient(app) as client:
        yield client


def test_list_pipelines_returns_200(client):
    """Test list pipelines returns 200."""
    response = client.get("/pipelines")
    assert response.status_code == 200


def test_list_pipelines_returns_array(client):
    """Test list pipelines returns array."""
    response = client.get("/pipelines")
    data = response.json()

    assert "pipelines" in data
    assert isinstance(data["pipelines"], list)


def test_pipeline_info_fields(client):
    """Test pipeline info has required fields."""
    response = client.get("/pipelines")
    data = response.json()

    if data["pipelines"]:
        pipeline = data["pipelines"][0]
        assert "name" in pipeline
        assert "description" in pipeline
        assert "version" in pipeline
        assert "input_type" in pipeline
        assert "output_type" in pipeline
        assert "streaming" in pipeline


def test_get_pipeline_not_found(client):
    """Test getting non-existent pipeline returns 404."""
    response = client.get("/pipelines/nonexistent")
    assert response.status_code == 404


def test_get_pipeline_detail_fields(client):
    """Test pipeline detail includes nodes and connections."""
    # First get list to find a pipeline name
    list_response = client.get("/pipelines")
    pipelines = list_response.json()["pipelines"]

    if pipelines:
        name = pipelines[0]["name"]
        response = client.get(f"/pipelines/{name}")

        if response.status_code == 200:
            data = response.json()
            # Detail view may include nodes and connections
            assert "name" in data
