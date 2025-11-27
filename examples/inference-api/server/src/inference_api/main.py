"""Main application entry point."""

import logging
from contextlib import asynccontextmanager
from typing import AsyncGenerator

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from inference_api.routes import health, pipelines, predict, stream
from inference_api.middleware.errors import error_handler
from inference_api.middleware.logging import LoggingMiddleware
from inference_api.services.registry import PipelineRegistry
from inference_api.services.sessions import SessionManager

logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncGenerator[None, None]:
    """Application lifespan manager."""
    # Startup
    logger.info("Starting Inference API server")

    # Initialize services
    app.state.pipeline_registry = PipelineRegistry()
    app.state.session_manager = SessionManager()

    # Load pipelines from directory
    app.state.pipeline_registry.load_from_directory("pipelines")

    yield

    # Shutdown
    logger.info("Shutting down Inference API server")
    await app.state.session_manager.close_all()


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    app = FastAPI(
        title="Inference API",
        description="REST API for RemoteMedia SDK pipeline execution",
        version="0.1.0",
        lifespan=lifespan,
    )

    # Add CORS middleware
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],  # Configure appropriately for production
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Add custom middleware
    app.add_middleware(LoggingMiddleware)

    # Add error handler
    app.add_exception_handler(Exception, error_handler)

    # Include routers
    app.include_router(health.router, tags=["Health"])
    app.include_router(pipelines.router, prefix="/pipelines", tags=["Pipelines"])
    app.include_router(predict.router, tags=["Prediction"])
    app.include_router(stream.router, prefix="/stream", tags=["Streaming"])

    return app


app = create_app()


def main() -> None:
    """Run the server."""
    import uvicorn

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    uvicorn.run(
        "inference_api.main:app",
        host="0.0.0.0",
        port=8000,
        reload=True,
    )


if __name__ == "__main__":
    main()
