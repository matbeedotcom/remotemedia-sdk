"""Authentication middleware."""

import logging
import os
from typing import Optional

from fastapi import HTTPException, Request, Security
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

logger = logging.getLogger(__name__)

security = HTTPBearer(auto_error=False)


def get_api_key() -> Optional[str]:
    """Get the API key from environment."""
    return os.environ.get("INFERENCE_API_KEY")


async def verify_token(
    request: Request,
    credentials: Optional[HTTPAuthorizationCredentials] = Security(security),
) -> Optional[str]:
    """Verify the bearer token.

    If INFERENCE_API_KEY is not set, authentication is disabled.
    """
    api_key = get_api_key()

    # Auth disabled if no key configured
    if api_key is None:
        return None

    # Auth required but not provided
    if credentials is None:
        raise HTTPException(
            status_code=401,
            detail="Authentication required",
            headers={"WWW-Authenticate": "Bearer"},
        )

    # Verify token
    if credentials.credentials != api_key:
        logger.warning(f"Invalid API key from {request.client}")
        raise HTTPException(
            status_code=401,
            detail="Invalid authentication token",
            headers={"WWW-Authenticate": "Bearer"},
        )

    return credentials.credentials
