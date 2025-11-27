"""SSE event formatting utilities."""

import json
from typing import Any


def format_sse_event(
    event: str,
    data: Any,
    event_id: str | None = None,
    retry: int | None = None,
) -> str:
    """Format data as an SSE event string.

    Args:
        event: Event type name
        data: Event data (will be JSON encoded if not a string)
        event_id: Optional event ID for client tracking
        retry: Optional retry timeout in milliseconds

    Returns:
        Formatted SSE event string
    """
    lines = []

    if event_id:
        lines.append(f"id: {event_id}")

    if retry is not None:
        lines.append(f"retry: {retry}")

    lines.append(f"event: {event}")

    # Encode data
    if isinstance(data, str):
        data_str = data
    else:
        data_str = json.dumps(data)

    # Handle multi-line data
    for line in data_str.split("\n"):
        lines.append(f"data: {line}")

    # SSE events must end with double newline
    return "\n".join(lines) + "\n\n"


def format_keepalive() -> str:
    """Format a keepalive comment.

    Keepalive comments prevent connection timeout.
    """
    return ": keepalive\n\n"


def format_error(
    code: str,
    message: str,
    recoverable: bool = True,
) -> str:
    """Format an error event."""
    return format_sse_event(
        "error",
        {
            "code": code,
            "message": message,
            "recoverable": recoverable,
        },
    )
