# Installation Guide for RemoteMedia Mono-repo

This mono-repo contains two main Python packages:
- `remotemedia.client`: The client SDK (in python-client/)
- `remotemedia.service`: The server/service implementation (in service/)

## Installation Options

### 1. Install Both Packages (Development)

From the repository root:

```bash
# Install the client SDK
pip install -e ./python-client

# Install the service package
pip install -e ./service
```

### 2. Install Client SDK Only

```bash
pip install -e ./python-client
```

### 3. Install Service Only

```bash
pip install -e ./service
```

### 4. Install with Development Dependencies

```bash
# Client SDK with dev dependencies
pip install -e "./python-client[dev]"

# Service with dev dependencies  
pip install -e "./service[dev]"
```

### 5. Install with ML Dependencies (Client SDK)

```bash
pip install -e "./python-client[ml]"
```

## Package Structure

- **remotemedia.client** (python-client/): Client SDK for interacting with remote media processing services
- **remotemedia.service** (service/): Backend service implementation for distributed processing

## Verifying Installation

```python
# Check client SDK
import remotemedia
print(remotemedia.__version__)

# Check shared protobuf definitions
from remotemedia.protos import execution_pb2
print("Protobuf definitions loaded successfully")

# Note: Service package can be imported, but some gRPC features may require compatible versions
```

## Console Scripts

After installation, the following commands will be available:
- `remotemedia-cli`: Client command-line interface
- `remotemedia-server`: Server executable

## Development Setup

For development, it's recommended to install both packages in editable mode (-e flag) so changes are immediately reflected without reinstalling.