"""Error handling middleware."""

import logging
import traceback
from typing import Any

from fastapi import Request
from fastapi.responses import JSONResponse

logger = logging.getLogger(__name__)


class APIError(Exception):
    """Base API error."""

    def __init__(
        self,
        code: str,
        message: str,
        status_code: int = 500,
        recoverable: bool = True,
        suggestion: str | None = None,
    ) -> None:
        self.code = code
        self.message = message
        self.status_code = status_code
        self.recoverable = recoverable
        self.suggestion = suggestion
        super().__init__(message)


class PipelineError(APIError):
    """Pipeline execution error."""

    def __init__(self, message: str, recoverable: bool = True) -> None:
        super().__init__(
            code="PIPELINE_ERROR",
            message=message,
            status_code=500,
            recoverable=recoverable,
            suggestion="Check the pipeline configuration and input data",
        )


class ValidationError(APIError):
    """Input validation error."""

    def __init__(self, message: str) -> None:
        super().__init__(
            code="VALIDATION_ERROR",
            message=message,
            status_code=400,
            recoverable=True,
            suggestion="Check the request format and data",
        )


class NotFoundError(APIError):
    """Resource not found error."""

    def __init__(self, resource: str, identifier: str) -> None:
        super().__init__(
            code="NOT_FOUND",
            message=f"{resource} '{identifier}' not found",
            status_code=404,
            recoverable=True,
            suggestion=f"Check that the {resource.lower()} exists",
        )


async def error_handler(request: Request, exc: Exception) -> JSONResponse:
    """Global error handler."""
    if isinstance(exc, APIError):
        return JSONResponse(
            status_code=exc.status_code,
            content={
                "code": exc.code,
                "message": exc.message,
                "recoverable": exc.recoverable,
                "suggestion": exc.suggestion,
            },
        )

    # Log unexpected errors
    logger.error(f"Unexpected error: {exc}")
    logger.debug(traceback.format_exc())

    return JSONResponse(
        status_code=500,
        content={
            "code": "INTERNAL_ERROR",
            "message": "An unexpected error occurred",
            "recoverable": False,
            "suggestion": "Please try again later or contact support",
        },
    )
