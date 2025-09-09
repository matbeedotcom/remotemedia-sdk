# Remote Execution Service

This directory contains the Docker-based remote execution service for the RemoteMedia SDK. The service provides a secure, sandboxed environment for executing processing nodes remotely via gRPC.

## Overview

The Remote Execution Service is designed to:
- Receive processing tasks via gRPC
- Execute SDK-defined nodes in a sandboxed environment
- Support user-defined Python code execution (Phase 3)
- Provide secure isolation and resource management
- Handle bidirectional streaming for real-time processing

## Architecture

```
remote_service/
├── Dockerfile                  # Main service container
├── docker-compose.yml         # Development environment
├── requirements.txt           # Service dependencies
├── src/                       # Service source code
│   ├── server.py              # Main gRPC server
│   ├── executor.py            # Task execution engine
│   ├── sandbox.py             # Sandboxing implementation
│   └── config.py              # Service configuration
├── protos/                    # gRPC protocol definitions
│   ├── execution.proto        # Main execution service
│   └── types.proto            # Common data types
├── scripts/                   # Utility scripts
│   ├── build.sh              # Build Docker image
│   ├── run.sh                # Run service locally
│   └── test.sh               # Test service
├── tests/                     # Service tests
│   ├── test_server.py        # gRPC server tests
│   ├── test_executor.py      # Execution engine tests
│   └── test_sandbox.py       # Sandboxing tests
└── config/                    # Configuration files
    ├── logging.yaml          # Logging configuration
    └── security.yaml         # Security policies
```

## Development Status

**Current Phase**: Phase 2 - Remote Execution for SDK Nodes

### Implemented Features
- ✅ Basic Docker container structure
- ✅ gRPC protocol definitions
- ✅ Server framework setup
- ⏳ Task execution engine
- ⏳ Sandboxing implementation
- ⏳ Security policies

### Planned Features (Phase 3)
- User-defined code execution
- Dynamic dependency installation
- Enhanced sandboxing with microVMs
- Resource monitoring and limits

## Quick Start

```bash
# Build the service
cd remote_service
./scripts/build.sh

# Run locally for development
./scripts/run.sh

# Run with docker-compose
docker-compose up -d

# Test the service
./scripts/test.sh
```

## Configuration

The service can be configured via environment variables or configuration files:

- `GRPC_PORT`: gRPC server port (default: 50051)
- `LOG_LEVEL`: Logging level (default: INFO)
- `SANDBOX_ENABLED`: Enable sandboxing (default: true)
- `MAX_WORKERS`: Maximum concurrent workers (default: 4)

## Security

The service implements multiple layers of security:
- Process-level sandboxing with restricted permissions
- Resource limits (CPU, memory, time)
- Network isolation
- File system restrictions
- Code validation and sanitization

## API

The service exposes a gRPC API defined in `protos/execution.proto`:

- `ExecuteNode`: Execute a predefined SDK node
- `ExecuteCustomTask`: Execute user-defined code (Phase 3)
- `StreamExecute`: Bidirectional streaming execution
- `GetStatus`: Service health and status

## Development

See the main project documentation for development guidelines and contribution instructions. 