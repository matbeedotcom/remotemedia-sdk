# Model Caching Configuration

The RemoteMedia remote service supports configurable caching for ML models to avoid re-downloading models every time containers are rebuilt or restarted.

## Cache Types

### 1. Hugging Face Models
- **Environment**: `HF_HOME`, `TRANSFORMERS_CACHE`
- **Default Path**: `/home/remotemedia/.cache/huggingface`
- **Purpose**: Caches transformer models, tokenizers, and datasets from Hugging Face Hub

### 2. PyTorch Models
- **Environment**: `TORCH_HOME`, `TORCH_CACHE`
- **Default Path**: `/home/remotemedia/.cache/torch`
- **Purpose**: Caches PyTorch models, checkpoints, and hub downloads

### 3. RemoteMedia Models
- **Environment**: `REMOTEMEDIA_CACHE_DIR`
- **Default Path**: `/app/cache/models`
- **Purpose**: Custom models and artifacts specific to the RemoteMedia SDK

## Configuration Options

### User ID Configuration

When using host directories, you can configure the container user to match your host user to avoid permission issues:

```bash
# In .env file or environment variables
USER_UID=1000  # Your user ID (get with: id -u)
USER_GID=1000  # Your group ID (get with: id -g)
```

This ensures that files created by the container will have the correct ownership for your host user.

### Option 1: Docker Volumes (Recommended for Production)

```yaml
volumes:
  - ml_model_cache:/app/cache/models
  - huggingface_cache:/home/remotemedia/.cache/huggingface
  - torch_cache:/home/remotemedia/.cache/torch
```

**Pros:**
- Managed by Docker
- Better isolation
- Automatic cleanup with `docker volume prune`

**Cons:**
- Less direct access to cached files
- Harder to pre-populate cache

### Option 2: Host Directories (Better for Development)

```yaml
volumes:
  - ./remote_service/cache/models:/app/cache/models
  - ./remote_service/cache/huggingface:/home/remotemedia/.cache/huggingface
  - ./remote_service/cache/torch:/home/remotemedia/.cache/torch
```

**Pros:**
- Direct access to cached files
- Easy to pre-populate or inspect cache
- Survives Docker system cleanup

**Cons:**
- Platform-specific paths
- Requires container rebuild when changing UID/GID
- Host filesystem dependencies

## Usage

### Using Docker Volumes
1. Start the service normally:
   ```bash
   docker compose up -d
   ```

2. Models will be cached automatically on first use
3. Cache persists across container restarts
4. View cache usage:
   ```bash
   docker volume ls | grep remote_service
   docker volume inspect remote_service_huggingface_cache
   ```

### Using Host Directories
1. Uncomment host directory mounts in `docker-compose.yml`
2. Comment out Docker volume mounts
3. Configure user ID to match your host user:
   ```bash
   # Copy the example environment file
   cp .env.example .env
   
   # Set your user ID and group ID (recommended)
   echo "USER_UID=$(id -u)" >> .env
   echo "USER_GID=$(id -g)" >> .env
   ```
4. Create cache directories:
   ```bash
   mkdir -p remote_service/cache/{models,huggingface,torch}
   ```
5. Start the service:
   ```bash
   docker compose up -d --build
   ```

**Note**: The `--build` flag is required when changing UID/GID to rebuild the container with the new user configuration.

**Important**: Due to Docker layer caching, you may need to use `--no-cache` when changing UID/GID:
```bash
docker compose build --no-cache
docker compose up -d
```

To verify the UID configuration worked:
```bash
# Check container user ID matches your host user
docker exec remotemedia-service id
id  # Compare with your host user ID
```

### Pre-populating Cache
For host directories, you can pre-download models:

```bash
# Example: Pre-download a Hugging Face model
python -c "
from transformers import AutoModel, AutoTokenizer
model_name = 'microsoft/DialoGPT-medium'
AutoModel.from_pretrained(model_name, cache_dir='./remote_service/cache/huggingface')
AutoTokenizer.from_pretrained(model_name, cache_dir='./remote_service/cache/huggingface')
"
```

## Monitoring Cache Usage

### Docker Volumes
```bash
# List all volumes
docker volume ls

# Inspect specific volume
docker volume inspect remote_service_huggingface_cache

# View volume usage (requires dive or similar tool)
docker run --rm -v remote_service_huggingface_cache:/cache busybox du -sh /cache
```

### Host Directories
```bash
# View cache size
du -sh remote_service/cache/

# List cached models
find remote_service/cache/ -name "*.bin" -o -name "*.safetensors" | head -10
```

## Cleanup

### Docker Volumes
```bash
# Remove unused volumes
docker volume prune

# Remove specific volume
docker volume rm remote_service_huggingface_cache
```

### Host Directories
```bash
# Clean specific cache
rm -rf remote_service/cache/huggingface/*

# Clean all caches
rm -rf remote_service/cache/
mkdir -p remote_service/cache/{models,huggingface,torch}
```

## Environment Variables

All cache paths can be customized via environment variables in `docker-compose.yml`:

```yaml
environment:
  - HF_HOME=/custom/hf/path
  - TRANSFORMERS_CACHE=/custom/hf/path
  - TORCH_HOME=/custom/torch/path
  - TORCH_CACHE=/custom/torch/path
  - REMOTEMEDIA_CACHE_DIR=/custom/models/path
```

Remember to update volume mounts accordingly when changing paths.