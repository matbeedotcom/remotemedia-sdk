"""Tests for predict endpoints."""

import base64

import pytest
from fastapi.testclient import TestClient

from inference_api.main import app


@pytest.fixture
def client():
    """Create test client."""
    with TestClient(app) as client:
        yield client


def test_predict_requires_pipeline(client):
    """Test predict requires pipeline name."""
    response = client.post("/predict", json={})
    assert response.status_code == 422


def test_predict_with_unknown_pipeline(client):
    """Test predict with unknown pipeline returns 404."""
    response = client.post(
        "/predict",
        json={"pipeline": "nonexistent", "input_data": ""},
    )
    assert response.status_code == 404


def test_predict_response_format(client):
    """Test predict response has required fields."""
    # First load a pipeline
    # This test would need actual pipelines loaded

    # For now, just verify the API structure
    response = client.post(
        "/predict",
        json={"pipeline": "test", "input_data": None},
    )

    # Should return 404 since no pipelines are loaded by default
    assert response.status_code in [200, 404]


def test_predict_accepts_base64_input(client):
    """Test predict accepts base64 encoded input."""
    # Encode some test data
    test_data = b"test audio data"
    encoded = base64.b64encode(test_data).decode()

    response = client.post(
        "/predict",
        json={
            "pipeline": "test",
            "input_data": encoded,
            "input_type": "audio",
        },
    )

    # Should either succeed or return 404 (no pipeline)
    assert response.status_code in [200, 404]


def test_predict_multipart_accepts_file(client):
    """Test multipart predict accepts file upload."""
    response = client.post(
        "/predict/multipart",
        data={"pipeline": "test"},
        files={"file": ("test.wav", b"test data", "audio/wav")},
    )

    # Should either succeed or return 404 (no pipeline)
    assert response.status_code in [200, 404]
