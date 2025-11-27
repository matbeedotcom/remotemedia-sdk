"""Request logging middleware."""

import logging
import time
from typing import Callable

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware

logger = logging.getLogger(__name__)


class LoggingMiddleware(BaseHTTPMiddleware):
    """Middleware for logging requests and responses."""

    async def dispatch(
        self, request: Request, call_next: Callable
    ) -> Response:
        # Skip health checks from logs to reduce noise
        if request.url.path == "/health":
            return await call_next(request)

        # Log request
        start_time = time.time()
        client = request.client.host if request.client else "unknown"

        logger.info(
            f"Request: {request.method} {request.url.path} "
            f"from {client}"
        )

        # Process request
        response = await call_next(request)

        # Log response
        duration_ms = (time.time() - start_time) * 1000

        logger.info(
            f"Response: {response.status_code} "
            f"in {duration_ms:.2f}ms"
        )

        # Add timing header
        response.headers["X-Response-Time"] = f"{duration_ms:.2f}ms"

        return response
