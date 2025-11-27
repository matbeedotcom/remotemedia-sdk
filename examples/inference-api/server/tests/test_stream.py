"""Tests for streaming endpoints."""

import pytest
from fastapi.testclient import TestClient

from inference_api.main import app


@pytest.fixture
def client():
    """Create test client."""
    with TestClient(app) as client:
        yield client


def test_start_stream_requires_pipeline(client):
    """Test start stream requires pipeline name."""
    response = client.post("/stream", json={})
    assert response.status_code == 422


def test_start_stream_unknown_pipeline(client):
    """Test start stream with unknown pipeline returns 404."""
    response = client.post("/stream", json={"pipeline": "nonexistent"})
    assert response.status_code == 404


def test_send_input_unknown_session(client):
    """Test sending input to unknown session returns 404."""
    response = client.post(
        "/stream/unknown-session-id/input",
        json={"input_data": None, "input_type": "audio"},
    )
    assert response.status_code == 404


def test_stream_output_unknown_session(client):
    """Test streaming output from unknown session returns 404."""
    response = client.get("/stream/unknown-session-id/output")
    assert response.status_code == 404


def test_close_stream_unknown_session(client):
    """Test closing unknown session returns 404."""
    response = client.delete("/stream/unknown-session-id")
    assert response.status_code == 404


# Integration tests that require actual pipeline would go here
# These are marked with pytest.mark.integration

@pytest.mark.skip(reason="Requires loaded pipeline")
def test_full_streaming_workflow(client):
    """Test complete streaming workflow."""
    # 1. Start session
    start_response = client.post("/stream", json={"pipeline": "test"})
    assert start_response.status_code == 200
    session_id = start_response.json()["session_id"]

    # 2. Send input
    input_response = client.post(
        f"/stream/{session_id}/input",
        json={"input_data": None, "input_type": "text"},
    )
    assert input_response.status_code == 200

    # 3. Close session
    close_response = client.delete(f"/stream/{session_id}")
    assert close_response.status_code == 200
