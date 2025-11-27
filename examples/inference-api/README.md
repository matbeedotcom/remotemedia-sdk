# Inference API

A REST API server for exposing RemoteMedia SDK pipelines as HTTP endpoints.

## Features

- **REST API**: Standard HTTP endpoints for pipeline execution
- **Streaming Support**: Server-Sent Events (SSE) for real-time output
- **Dashboard**: Web UI for testing and monitoring
- **OpenAPI Documentation**: Auto-generated API documentation
- **Authentication**: Optional bearer token authentication

## Quick Start

### Server

```bash
cd server

# Install dependencies
pip install -e .

# Run server
inference-api
# or
uvicorn inference_api.main:app --reload
```

Server starts at http://localhost:8000

### Dashboard

```bash
cd dashboard

# Install dependencies
npm install

# Run development server
npm run dev
```

Dashboard starts at http://localhost:5173

## API Endpoints

### Health

```
GET /health
```

Returns service health status.

### Pipelines

```
GET /pipelines
GET /pipelines/{name}
```

List and inspect available pipelines.

### Prediction (Unary)

```
POST /predict
Content-Type: application/json

{
  "pipeline": "whisper-transcribe",
  "input_data": "<base64-encoded-audio>",
  "input_type": "audio"
}
```

Execute a pipeline and get the result.

```
POST /predict/multipart
Content-Type: multipart/form-data

pipeline=whisper-transcribe
file=@audio.wav
```

Upload files directly.

### Streaming

```
POST /stream
{
  "pipeline": "voice-assistant"
}
→ { "session_id": "...", "status": "active" }

POST /stream/{session_id}/input
{
  "input_data": "<base64>",
  "input_type": "audio"
}

GET /stream/{session_id}/output
→ SSE stream

DELETE /stream/{session_id}
```

## Configuration

### Environment Variables

- `INFERENCE_API_KEY`: Bearer token for authentication (optional)
- `INFERENCE_API_HOST`: Server host (default: 0.0.0.0)
- `INFERENCE_API_PORT`: Server port (default: 8000)

### Pipelines

Place pipeline YAML files in `pipelines/` directory:

```yaml
# pipelines/my-pipeline.yaml
name: my-pipeline
version: "1.0"
description: My custom pipeline

nodes:
  - id: input
    node_type: TextInput
  - id: process
    node_type: EchoNode
  - id: output
    node_type: TextOutput

connections:
  - from: input
    to: process
  - from: process
    to: output
```

## Development

### Running Tests

```bash
cd server
pytest
```

### Load Testing

```bash
cd server
locust -f tests/test_load.py
```

### OpenAPI Spec

Access at http://localhost:8000/docs (Swagger UI) or http://localhost:8000/redoc

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Dashboard                         │
│                   (Vite + React)                     │
└────────────────────────┬────────────────────────────┘
                         │ HTTP/SSE
┌────────────────────────▼────────────────────────────┐
│                   FastAPI Server                     │
│                                                      │
│  /health  /pipelines  /predict  /stream             │
│                                                      │
│  ┌──────────────────────────────────────────────┐  │
│  │              Service Layer                    │  │
│  │  Registry │ Executor │ Sessions │ SSE        │  │
│  └──────────────────────────────────────────────┘  │
│                         │                           │
│  ┌──────────────────────▼──────────────────────┐   │
│  │           RemoteMedia Runtime (FFI)          │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

## License

Apache-2.0
