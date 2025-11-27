"""Pipeline management endpoints."""

from fastapi import APIRouter, HTTPException, Request

from inference_api.models.responses import PipelineInfo, PipelineListResponse

router = APIRouter()


@router.get("", response_model=PipelineListResponse)
async def list_pipelines(request: Request) -> PipelineListResponse:
    """List all available pipelines."""
    registry = request.app.state.pipeline_registry
    pipelines = registry.list_pipelines()

    return PipelineListResponse(
        pipelines=[
            PipelineInfo(
                name=p["name"],
                description=p.get("description", ""),
                version=p.get("version", "1.0"),
                input_type=p.get("input_type", "unknown"),
                output_type=p.get("output_type", "unknown"),
                streaming=p.get("streaming", False),
            )
            for p in pipelines
        ]
    )


@router.get("/{name}", response_model=PipelineInfo)
async def get_pipeline(name: str, request: Request) -> PipelineInfo:
    """Get details of a specific pipeline."""
    registry = request.app.state.pipeline_registry
    pipeline = registry.get_pipeline(name)

    if pipeline is None:
        raise HTTPException(status_code=404, detail=f"Pipeline '{name}' not found")

    return PipelineInfo(
        name=pipeline["name"],
        description=pipeline.get("description", ""),
        version=pipeline.get("version", "1.0"),
        input_type=pipeline.get("input_type", "unknown"),
        output_type=pipeline.get("output_type", "unknown"),
        streaming=pipeline.get("streaming", False),
        nodes=pipeline.get("nodes", []),
        connections=pipeline.get("connections", []),
    )
