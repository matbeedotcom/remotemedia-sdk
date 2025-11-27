"""Load tests for concurrent requests.

Run with: locust -f tests/test_load.py
"""

import base64
import random

from locust import HttpUser, between, task


class InferenceAPIUser(HttpUser):
    """Simulated user for load testing."""

    wait_time = between(0.5, 2.0)

    @task(10)
    def health_check(self):
        """Check health endpoint (high frequency)."""
        self.client.get("/health")

    @task(5)
    def list_pipelines(self):
        """List available pipelines."""
        self.client.get("/pipelines")

    @task(3)
    def predict_text(self):
        """Make text prediction."""
        text = f"Test input {random.randint(1, 1000)}"
        encoded = base64.b64encode(text.encode()).decode()

        self.client.post(
            "/predict",
            json={
                "pipeline": "echo",  # Assuming an echo pipeline exists
                "input_data": encoded,
                "input_type": "text",
            },
        )

    @task(1)
    def streaming_session(self):
        """Create and close a streaming session."""
        # Start session
        start_response = self.client.post(
            "/stream",
            json={"pipeline": "echo"},
        )

        if start_response.status_code == 200:
            session_id = start_response.json()["session_id"]

            # Send some input
            for _ in range(3):
                self.client.post(
                    f"/stream/{session_id}/input",
                    json={
                        "input_data": base64.b64encode(b"test").decode(),
                        "input_type": "text",
                    },
                )

            # Close session
            self.client.delete(f"/stream/{session_id}")


class HighLoadUser(HttpUser):
    """High-load user for stress testing."""

    wait_time = between(0.1, 0.5)

    @task
    def rapid_health_check(self):
        """Rapid health checks."""
        self.client.get("/health")
