# Remote Node Configuration Schema

**Contract Version**: v0.4.0  
**Feature**: 006-remote-pipeline-node

## JSON Schema

Node type: `"RemotePipeline"`

### Required Fields

- **transport**: string, one of `"grpc"`, `"webrtc"`, `"http"`
- **endpoints**: array of strings (URLs), minimum 1 element
- **manifest_source**: object (see ManifestSource schema below)

### Optional Fields

- **timeout_ms**: integer, default 30000, range [1000, 300000]
- **retry**: object (see RetryConfig schema)
- **circuit_breaker**: object (see CircuitBreakerConfig schema)
- **load_balance**: string, one of `"round_robin"`, `"least_connections"`, `"random"`, default `"round_robin"`
- **health_check_interval_secs**: integer, default 5, range [1, 60]
- **auth_token**: string (supports env var substitution: `${VAR_NAME}`)

## ManifestSource Schema

Tagged union with `type` discriminator:

### Inline Variant
```json
{
  "type": "inline",
  "manifest": "<JSON string of pipeline manifest>"
}
```

### URL Variant
```json
{
  "type": "url",
  "url": "https://example.com/manifest.json",
  "auth_header": "Bearer token123" (optional)
}
```

### Name Variant
```json
{
  "type": "name",
  "name": "pipeline-identifier"
}
```

## RetryConfig Schema

All fields optional with defaults:

```json
{
  "max_attempts": 3,
  "initial_backoff_ms": 1000,
  "max_backoff_ms": 30000,
  "multiplier": 2.0
}
```

## CircuitBreakerConfig Schema

All fields optional with defaults:

```json
{
  "failure_threshold": 5,
  "reset_timeout_ms": 30000
}
```

## Complete Example

```json
{
  "id": "remote_tts_node",
  "node_type": "RemotePipeline",
  "params": {
    "transport": "grpc",
    "endpoints": [
      "https://tts-primary.example.com:50051",
      "https://tts-secondary.example.com:50051"
    ],
    "manifest_source": {
      "type": "url",
      "url": "https://manifests.example.com/tts-v2.json",
      "auth_header": "Bearer ${MANIFEST_TOKEN}"
    },
    "timeout_ms": 45000,
    "retry": {
      "max_attempts": 5,
      "initial_backoff_ms": 500
    },
    "circuit_breaker": {
      "failure_threshold": 3
    },
    "load_balance": "least_connections",
    "health_check_interval_secs": 10,
    "auth_token": "${TTS_API_TOKEN}"
  }
}
```

## Validation Rules

1. At least one endpoint must be provided
2. timeout_ms must be > 0
3. retry.max_attempts must be >= 0
4. circuit_breaker.failure_threshold must be > 0
5. Environment variable substitution occurs at manifest parse time
6. Invalid transport type results in manifest validation error

## Error Codes

- `ERR_INVALID_TRANSPORT`: Unknown transport type
- `ERR_NO_ENDPOINTS`: Empty endpoints array
- `ERR_INVALID_TIMEOUT`: timeout_ms out of range
- `ERR_INVALID_MANIFEST_SOURCE`: Malformed manifest_source object
- `ERR_ENV_VAR_NOT_FOUND`: Referenced env var does not exist

---

**Status**: ✅ Stable
