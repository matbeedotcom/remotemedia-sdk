# Control Message Wire Format Specification

**Feature**: 007-low-latency-streaming | **Version**: 1.0.0 | **Date**: 2025-11-10

## Overview

This document defines the wire format for control messages transmitted across all execution contexts: local Rust channels, multiprocess IPC (iceoryx2), and remote transports (gRPC, WebRTC, HTTP).

---

## Binary Format (IPC via iceoryx2)

Extends the existing RuntimeData binary format with a new data type.

### Wire Format

```
┌─────────────────────────────────────────────────────────────┐
│ Type (1 byte) = 5 (ControlMessage)                          │
├─────────────────────────────────────────────────────────────┤
│ Session Length (2 bytes, little-endian)                     │
├─────────────────────────────────────────────────────────────┤
│ Session ID (UTF-8 string, variable length)                  │
├─────────────────────────────────────────────────────────────┤
│ Timestamp (8 bytes, little-endian, microseconds since epoch)│
├─────────────────────────────────────────────────────────────┤
│ Payload Length (4 bytes, little-endian)                     │
├─────────────────────────────────────────────────────────────┤
│ Payload (JSON-encoded ControlMessage, variable length)      │
└─────────────────────────────────────────────────────────────┘
```

### Data Type Enum Extension

```rust
#[repr(u8)]
pub enum DataType {
    Audio = 1,
    Video = 2,
    Text = 3,
    Tensor = 4,
    ControlMessage = 5,  // NEW
}
```

### Payload JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ControlMessage",
  "type": "object",
  "required": ["message_type", "session_id", "timestamp"],
  "properties": {
    "message_type": {
      "oneOf": [
        {
          "type": "object",
          "properties": {
            "CancelSpeculation": {
              "type": "object",
              "required": ["from_timestamp", "to_timestamp"],
              "properties": {
                "from_timestamp": { "type": "integer", "minimum": 0 },
                "to_timestamp": { "type": "integer", "minimum": 0 }
              }
            }
          }
        },
        {
          "type": "object",
          "properties": {
            "BatchHint": {
              "type": "object",
              "required": ["suggested_batch_size"],
              "properties": {
                "suggested_batch_size": { "type": "integer", "minimum": 1 }
              }
            }
          }
        },
        {
          "type": "object",
          "properties": {
            "DeadlineWarning": {
              "type": "object",
              "required": ["deadline_us"],
              "properties": {
                "deadline_us": { "type": "integer", "minimum": 0 }
              }
            }
          }
        }
      ]
    },
    "session_id": { "type": "string" },
    "timestamp": { "type": "integer", "minimum": 0 },
    "target_segment_id": { "type": ["string", "null"] },
    "metadata": { "type": "object" }
  }
}
```

### Example Binary Encoding

**CancelSpeculation Message**:
```
Input:
{
  "message_type": { "CancelSpeculation": { "from_timestamp": 123456, "to_timestamp": 123789 } },
  "session_id": "sess_abc123",
  "timestamp": 1700000000000,
  "target_segment_id": "550e8400-e29b-41d4-a716-446655440000",
  "metadata": {}
}

Binary (hex):
05                                  # Type = ControlMessage (5)
0A 00                              # Session length = 10
73 65 73 73 5F 61 62 63 31 32 33  # "sess_abc123"
00 D0 7F 26 3C 8C 01 00           # Timestamp = 1700000000000
XX XX XX XX                        # Payload length (calculated)
{ ... JSON payload ... }           # Full JSON object
```

---

## gRPC Format (Protobuf)

Add new message type to existing pipeline protobuf schema.

### Protobuf Definition

```protobuf
syntax = "proto3";

package remotemedia.pipeline;

// Existing RuntimeData message
message RuntimeData {
  oneof data {
    AudioData audio = 1;
    VideoData video = 2;
    TextData text = 3;
    TensorData tensor = 4;
    ControlMessage control = 5;  // NEW
  }
  string session_id = 10;
  uint64 timestamp = 11;
}

// NEW: Control message
message ControlMessage {
  oneof message_type {
    CancelSpeculation cancel_speculation = 1;
    BatchHint batch_hint = 2;
    DeadlineWarning deadline_warning = 3;
  }
  string session_id = 10;
  uint64 timestamp = 11;
  optional string target_segment_id = 12;
  string metadata_json = 13;  // JSON string for extensibility
}

message CancelSpeculation {
  uint64 from_timestamp = 1;
  uint64 to_timestamp = 2;
}

message BatchHint {
  uint32 suggested_batch_size = 1;
}

message DeadlineWarning {
  uint64 deadline_us = 1;
}
```

### gRPC Service Extension

No service changes required - control messages flow through existing `StreamPipeline` bidirectional stream:

```protobuf
service Pipeline {
  rpc StreamPipeline(stream RuntimeData) returns (stream RuntimeData);
}
```

---

## WebRTC Format (Data Channel)

Control messages sent over WebRTC data channel as JSON.

### JSON Format

```json
{
  "type": "control_message",
  "message_type": {
    "CancelSpeculation": {
      "from_timestamp": 123456,
      "to_timestamp": 123789
    }
  },
  "session_id": "sess_abc123",
  "timestamp": 1700000000000,
  "target_segment_id": "550e8400-e29b-41d4-a716-446655440000",
  "metadata": {}
}
```

### Data Channel Configuration

```rust
// Create reliable, ordered data channel for control messages
let control_channel = RTCDataChannel::new(
    "control",
    RTCDataChannelInit {
        ordered: true,
        max_retransmits: Some(3),
        ..Default::default()
    }
);
```

---

## HTTP/SSE Format (Server-Sent Events)

Control messages sent as SSE events.

### SSE Event Format

```
event: control_message
data: {"message_type":{"CancelSpeculation":{"from_timestamp":123456,"to_timestamp":123789}},"session_id":"sess_abc123","timestamp":1700000000000,"target_segment_id":"550e8400-e29b-41d4-a716-446655440000","metadata":{}}

```

### HTTP Request (Client to Server)

Control messages from client sent as POST to `/control`:

```http
POST /control HTTP/1.1
Content-Type: application/json

{
  "message_type": { "BatchHint": { "suggested_batch_size": 5 } },
  "session_id": "sess_abc123",
  "timestamp": 1700000000000,
  "metadata": {}
}
```

---

## Python Deserialization (for multiprocess nodes)

Python nodes receive control messages via iceoryx2 and deserialize from binary format.

### Python Code

```python
import json
import struct
from typing import Optional

class ControlMessageType:
    CANCEL_SPECULATION = "CancelSpeculation"
    BATCH_HINT = "BatchHint"
    DEADLINE_WARNING = "DeadlineWarning"

def deserialize_control_message(data: bytes) -> dict:
    """
    Deserialize control message from iceoryx2 binary format.

    Format: type (1) | session_len (2) | session_id | timestamp (8) | payload_len (4) | payload (JSON)
    """
    if len(data) < 15:
        raise ValueError("Data too short for control message")

    pos = 0

    # Type byte
    data_type = data[pos]
    if data_type != 5:  # ControlMessage
        raise ValueError(f"Invalid data type: {data_type}, expected 5")
    pos += 1

    # Session ID
    session_len = struct.unpack('<H', data[pos:pos+2])[0]
    pos += 2
    session_id = data[pos:pos+session_len].decode('utf-8')
    pos += session_len

    # Timestamp
    timestamp = struct.unpack('<Q', data[pos:pos+8])[0]
    pos += 8

    # Payload
    payload_len = struct.unpack('<I', data[pos:pos+4])[0]
    pos += 4
    payload_json = data[pos:pos+payload_len].decode('utf-8')

    # Parse JSON payload
    payload = json.loads(payload_json)
    payload['session_id'] = session_id
    payload['timestamp'] = timestamp

    return payload

def handle_control_message(node, message: dict):
    """Handle control message in Python node."""
    msg_type = next(iter(message['message_type'].keys()))

    if msg_type == ControlMessageType.CANCEL_SPECULATION:
        # Terminate processing for segment in range
        from_ts = message['message_type'][msg_type]['from_timestamp']
        to_ts = message['message_type'][msg_type]['to_timestamp']
        node.cancel_segment(from_ts, to_ts)

    elif msg_type == ControlMessageType.BATCH_HINT:
        # Adjust batching behavior
        batch_size = message['message_type'][msg_type]['suggested_batch_size']
        node.update_batch_size(batch_size)

    elif msg_type == ControlMessageType.DEADLINE_WARNING:
        # Adjust quality for deadline
        deadline_us = message['message_type'][msg_type]['deadline_us']
        node.adjust_for_deadline(deadline_us)
```

---

## Validation & Error Handling

### Validation Rules

All implementations must validate:
1. **Type byte** = 5 for ControlMessage
2. **Session ID** matches current session (warn if mismatch, don't drop)
3. **Timestamp** is recent (within last 1 second, warn if stale)
4. **Payload JSON** is valid and conforms to schema
5. For `CancelSpeculation`: `from_timestamp` < `to_timestamp`

### Error Handling

| Error | Action |
|-------|--------|
| Invalid type byte | Drop message, log error |
| Session ID mismatch | Log warning, process anyway (may be valid for multi-session node) |
| Stale timestamp (>1s old) | Log warning, process (may be network delay) |
| Invalid JSON | Drop message, log error |
| Unknown message_type | Log warning, ignore (forward compatibility) |

---

## Versioning

- **Version**: 1.0.0 (initial release)
- **Compatibility**: Forward-compatible via `metadata` field and unknown `message_type` variants
- **Breaking Changes**: Require major version bump (2.0.0)

### Future Extensions

To add new control message types:
1. Add variant to `ControlMessageType` enum
2. Update JSON schema
3. Update protobuf definition
4. Update Python deserialization
5. Maintain backward compatibility (old nodes ignore unknown types)

---

## Performance Considerations

- **Binary format overhead**: ~20-50 bytes (fixed headers) + JSON payload
- **Serialization**: <10μs for typical control message
- **Propagation latency**: Target <10ms P95 across all transports
- **No zero-copy**: Control messages are infrequent (<10/sec), copy overhead acceptable

---

## Summary

✅ **Unified format** across all execution contexts (local, IPC, gRPC, WebRTC, HTTP)
✅ **Extensible** via JSON metadata and enum variants
✅ **Validated** with clear error handling rules
✅ **Versioned** for future compatibility
✅ **Python-compatible** with deserialization code provided

**Ready for implementation.**
