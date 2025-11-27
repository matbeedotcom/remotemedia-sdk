"""Prediction endpoints for unary execution."""

import base64
from typing import Optional

from fastapi import APIRouter, File, Form, HTTPException, Request, UploadFile

from inference_api.models.requests import PredictRequest
from inference_api.models.responses import PredictResponse
from inference_api.services.executor import PipelineExecutor

router = APIRouter()


@router.post("/predict", response_model=PredictResponse)
async def predict_json(request: Request, body: PredictRequest) -> PredictResponse:
    """Execute a pipeline with JSON input.

    Input data should be base64 encoded if binary (audio, image).
    """
    registry = request.app.state.pipeline_registry
    pipeline = registry.get_pipeline(body.pipeline)

    if pipeline is None:
        raise HTTPException(status_code=404, detail=f"Pipeline '{body.pipeline}' not found")

    # Decode input data
    input_data = base64.b64decode(body.input_data) if body.input_data else None

    # Execute pipeline
    executor = PipelineExecutor(pipeline)
    try:
        result = await executor.execute(input_data, body.config)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

    # Encode output if binary
    output_data = base64.b64encode(result.data).decode() if result.data else None

    return PredictResponse(
        output_type=result.output_type,
        output_data=output_data,
        metadata=result.metadata,
        timing_ms=result.timing_ms,
    )


@router.post("/predict/multipart", response_model=PredictResponse)
async def predict_multipart(
    request: Request,
    pipeline: str = Form(...),
    file: UploadFile = File(...),
    config: Optional[str] = Form(None),
) -> PredictResponse:
    """Execute a pipeline with multipart file upload.

    Useful for uploading audio/video files directly.
    """
    registry = request.app.state.pipeline_registry
    pipeline_def = registry.get_pipeline(pipeline)

    if pipeline_def is None:
        raise HTTPException(status_code=404, detail=f"Pipeline '{pipeline}' not found")

    # Read file content
    input_data = await file.read()

    # Parse config if provided
    import json

    config_dict = json.loads(config) if config else None

    # Execute pipeline
    executor = PipelineExecutor(pipeline_def)
    try:
        result = await executor.execute(input_data, config_dict)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

    # Encode output if binary
    output_data = base64.b64encode(result.data).decode() if result.data else None

    return PredictResponse(
        output_type=result.output_type,
        output_data=output_data,
        metadata=result.metadata,
        timing_ms=result.timing_ms,
    )
