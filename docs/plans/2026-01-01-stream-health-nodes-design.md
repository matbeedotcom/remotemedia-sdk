# Stream Health Monitoring Nodes Design

**Date:** 2026-01-01
**Status:** Approved
**Author:** Claude (Opus 4.5)

## Overview

This document specifies six new streaming nodes for comprehensive stream health monitoring, designed for contact center QA and real-time communications use cases.

### Node Summary

| Layer | Node | Purpose |
|-------|------|---------|
| Business | `SpeechPresenceNode` | Semantic speech states (speaking/silent/dead_air/overlap) |
| Business | `ConversationFlowNode` | Talk ratios and silence percentages over sliding windows |
| Business | `SessionHealthNode` | Aggregated health status (ok/degraded/unhealthy) |
| Technical | `TimingDriftNode` | PTS vs arrival timing analysis (jitter, drift) |
| Technical | `EventCorrelatorNode` | Groups related alerts into incidents |
| Technical | `AudioEvidenceNode` | Rolling audio buffer for alert evidence capture |

## Architecture

### Data Flow

```
Audio → SileroVAD → SpeechPresence → ConversationFlow
           ↓              ↓               ↓
       AudioLevel    Clipping    ChannelBalance    HealthEmitter
           ↓              ↓               ↓              ↓
           └──────────────┴───────────────┴──────────────┘
                              ↓
                       SessionHealth ─────→ EventCorrelator
                              ↓
                       (on alert) → AudioEvidence
```

### Channel Handling

All nodes support both stereo and mono audio:

- **Stereo (channels=2):** Run per-channel analysis, enable overlap detection, compute per-speaker metrics
- **Mono (channels=1):** Graceful fallback to aggregate metrics only

### Output Format

All nodes emit `RuntimeData::Json` with consistent structure:

```json
{
  "event_type": "node.category",
  "_schema": "event_type_v1",
  "timestamp_us": 1234567890,
  "stream_id": "optional"
}
```

---

## Business Layer Nodes

### 1. SpeechPresenceNode

**Purpose:** Converts raw VAD signals into semantic speech states with duration tracking.

**Input:** `RuntimeData::Audio` (runs internal VAD) or downstream of `SileroVADNode` JSON events.

#### Configuration

```rust
pub struct SpeechPresenceConfig {
    /// Duration (ms) of silence before classifying as "dead_air"
    pub dead_air_threshold_ms: u32,        // default: 2000

    /// Duration (ms) of simultaneous speech before classifying as "overlap" (stereo only)
    pub overlap_threshold_ms: u32,         // default: 500

    /// VAD speech probability threshold (if running internal VAD)
    pub vad_threshold: f32,                // default: 0.5

    /// Emit periodic updates even without state change
    pub emit_interval_ms: Option<u32>,     // default: None (emit on change only)
}
```

#### States

**Stereo:**
| State | Condition |
|-------|-----------|
| `speaking_left` | Left channel VAD active, right inactive |
| `speaking_right` | Right channel VAD active, left inactive |
| `speaking_both` | Both channels active < overlap_threshold |
| `overlap` | Both channels active >= overlap_threshold |
| `silent` | Neither channel active < dead_air_threshold |
| `dead_air` | Neither channel active >= dead_air_threshold |

**Mono:**
| State | Condition |
|-------|-----------|
| `speaking` | VAD active |
| `silent` | VAD inactive < dead_air_threshold |
| `dead_air` | VAD inactive >= dead_air_threshold |

#### Output (Stereo)

```json
{
  "event_type": "speech.presence",
  "state": "overlap",
  "speakers": {
    "left": {"active": true, "duration_ms": 1200},
    "right": {"active": true, "duration_ms": 800}
  },
  "overlap_duration_ms": 500,
  "timestamp_us": 1234567890,
  "stream_id": "call_123"
}
```

#### Output (Mono)

```json
{
  "event_type": "speech.presence",
  "state": "dead_air",
  "duration_ms": 2800,
  "timestamp_us": 1234567890
}
```

---

### 2. ConversationFlowNode

**Purpose:** Computes talk ratios and silence percentages over sliding time windows.

**Input:** Consumes `SpeechPresenceNode` events.

#### Configuration

```rust
pub struct ConversationFlowConfig {
    /// Sliding window size in milliseconds
    pub window_ms: u32,                    // default: 30000 (30s)

    /// How often to emit flow metrics
    pub emit_interval_ms: u32,             // default: 5000 (5s)
}
```

#### Output (Stereo)

```json
{
  "event_type": "conversation.flow",
  "left_talk_pct": 65.2,
  "right_talk_pct": 28.1,
  "silence_pct": 6.7,
  "overlap_pct": 3.4,
  "window_ms": 30000,
  "timestamp_us": 1234567890
}
```

#### Output (Mono)

```json
{
  "event_type": "conversation.flow",
  "talk_pct": 78.5,
  "silence_pct": 21.5,
  "window_ms": 30000,
  "timestamp_us": 1234567890
}
```

#### Implementation Notes

- Uses ring buffer of presence events within window
- Percentages calculated from cumulative durations
- Overlap is counted once (not double-counted in both speaker ratios)

---

### 3. SessionHealthNode

**Purpose:** Aggregates multiple signal sources into a single health status for UI display.

**Input:** Multi-input node consuming events from:
- `SpeechPresenceNode` (dead_air detection)
- `SilenceDetectorNode` (sustained silence)
- `ClippingDetectorNode` (is_clipping)
- `AudioLevelNode` (is_low_volume, is_silence)
- `ChannelBalanceNode` (is_imbalanced, has_dead_channel)
- `HealthEmitterNode` (drift, freeze, health_score)

#### Configuration

```rust
pub struct SessionHealthConfig {
    /// Emit interval in milliseconds
    pub emit_interval_ms: u32,             // default: 1000

    /// Health score threshold for "degraded"
    pub degraded_threshold: f64,           // default: 0.8

    /// Health score threshold for "unhealthy"
    pub unhealthy_threshold: f64,          // default: 0.5

    /// Dead air duration (ms) to count as issue
    pub dead_air_issue_ms: u32,            // default: 3000
}
```

#### States

| State | Condition |
|-------|-----------|
| `ok` | Health score >= degraded_threshold, no critical issues |
| `degraded` | Health score >= unhealthy_threshold OR 1-2 minor issues |
| `unhealthy` | Health score < unhealthy_threshold OR any critical issue |

#### Contributors Tracked

- `silence` - Sustained silence detected
- `dead_air` - Dead air >= threshold
- `clipping` - Audio clipping detected
- `low_volume` - Consistently low volume
- `imbalance` - Channel imbalance
- `drift` - Timing drift alert
- `freeze` - Content freeze detected

#### Output

```json
{
  "event_type": "session.health",
  "state": "degraded",
  "score": 0.72,
  "contributors": ["silence", "low_volume"],
  "active_issues": 2,
  "timestamp_us": 1234567890
}
```

---

## Technical Layer Nodes

### 4. TimingDriftNode

**Purpose:** Exposes timing metrics from `DriftMetrics` as streaming node events. Provides jitter and drift analysis for infrastructure debugging.

**Input:** `RuntimeData::Audio` or `RuntimeData::Video` with `timestamp_us` and `arrival_ts_us` fields.

**Note:** This is largely a wrapper around the existing `DriftMetrics` infrastructure.

#### Configuration

```rust
pub struct TimingDriftConfig {
    /// Jitter spike threshold in milliseconds
    pub jitter_threshold_ms: u32,          // default: 50

    /// Clock drift threshold in ms/s
    pub drift_threshold_ms_per_s: f64,     // default: 5.0

    /// Emit interval for periodic timing reports
    pub emit_interval_ms: u32,             // default: 1000
}
```

#### Output (Periodic Report)

```json
{
  "event_type": "timing.report",
  "lead_ms": 23.5,
  "slope_ms_per_s": 1.2,
  "jitter_ms": 8.3,
  "cadence_cv": 0.12,
  "timestamp_us": 1234567890
}
```

#### Output (Jitter Alert)

```json
{
  "event_type": "timing.jitter_spike",
  "jitter_ms": 85.0,
  "threshold_ms": 50,
  "timestamp_us": 1234567890
}
```

#### Output (Drift Alert)

```json
{
  "event_type": "timing.clock_drift",
  "slope_ms_per_s": 8.5,
  "threshold_ms_per_s": 5.0,
  "timestamp_us": 1234567890
}
```

---

### 5. EventCorrelatorNode

**Purpose:** Groups temporally-related alerts into incidents, reducing alert spam and adding context.

**Input:** Consumes JSON events from any alert-producing node.

#### Configuration

```rust
pub struct EventCorrelatorConfig {
    /// Time window (ms) to correlate events into same incident
    pub correlation_window_ms: u32,        // default: 5000

    /// Minimum events to form an incident
    pub min_events_for_incident: u32,      // default: 2

    /// Emit individual events as well as incidents
    pub emit_raw_events: bool,             // default: true
}
```

#### Correlation Logic

1. Collect events within `correlation_window_ms`
2. If >= `min_events_for_incident` events occur, group as incident
3. Assign incident severity based on worst event
4. Add context about event sequence

#### Patterns Detected

- `silence_then_reconnect` - Silence followed by timing reset
- `clipping_with_imbalance` - Audio distortion with channel issues
- `freeze_with_drift` - Content freeze correlating with timing drift

#### Output (Incident)

```json
{
  "event_type": "incident",
  "incident_id": "inc_abc123",
  "severity": "high",
  "events": [
    {"type": "silence", "timestamp_us": 1234567000},
    {"type": "clipping", "timestamp_us": 1234567500},
    {"type": "freeze", "timestamp_us": 1234568000}
  ],
  "pattern": "silence_then_clipping",
  "duration_ms": 1500,
  "timestamp_us": 1234567890
}
```

---

### 6. AudioEvidenceNode

**Purpose:** Maintains rolling audio buffer and exports clips when alerts occur.

**Input:** `RuntimeData::Audio` for buffering, alert events as trigger.

**Important:** This node is opt-in and privacy-sensitive. Buffer contents are never persisted automatically.

#### Configuration

```rust
pub struct AudioEvidenceConfig {
    /// Rolling buffer duration in seconds
    pub buffer_duration_s: u32,            // default: 10

    /// Pre-alert clip duration (seconds before alert)
    pub pre_alert_s: u32,                  // default: 3

    /// Post-alert clip duration (seconds after alert)
    pub post_alert_s: u32,                 // default: 2

    /// Maximum clips to retain per session
    pub max_clips: u32,                    // default: 10

    /// Whether to emit clips or just references
    pub emit_audio_data: bool,             // default: false (emit reference only)
}
```

#### Output (Clip Reference)

```json
{
  "event_type": "audio.evidence",
  "clip_id": "clip_xyz789",
  "trigger_event": "clipping",
  "start_offset_ms": -3000,
  "duration_ms": 5000,
  "sample_rate": 16000,
  "timestamp_us": 1234567890
}
```

#### Output (With Audio Data)

```json
{
  "event_type": "audio.evidence",
  "clip_id": "clip_xyz789",
  "trigger_event": "clipping",
  "audio_base64": "...",
  "sample_rate": 16000,
  "channels": 1,
  "duration_ms": 5000,
  "timestamp_us": 1234567890
}
```

#### Implementation Notes

- Uses ring buffer sized to `buffer_duration_s * sample_rate`
- Clips extracted on alert event receipt
- Memory-bounded by `max_clips * clip_duration * sample_rate * sizeof(f32)`

---

## Implementation Plan

### Order

1. **SpeechPresenceNode** - Foundation for conversation metrics
2. **SessionHealthNode** - The "status light" everyone wants
3. **ConversationFlowNode** - Talk ratios for QA story
4. **TimingDriftNode** - Technical drill-down
5. **EventCorrelatorNode** - Alert intelligence
6. **AudioEvidenceNode** - Evidence capture (optional)

### File Structure

```
runtime-core/src/nodes/
├── speech_presence.rs       # SpeechPresenceNode
├── conversation_flow.rs     # ConversationFlowNode
├── session_health.rs        # SessionHealthNode
├── timing_drift.rs          # TimingDriftNode
├── event_correlator.rs      # EventCorrelatorNode
├── audio_evidence.rs        # AudioEvidenceNode
└── mod.rs                   # Updated with new exports
```

### Registration

All nodes will be registered in `streaming_registry.rs` with their factories.

---

## Testing Strategy

Each node will have:
1. Unit tests for core logic (state transitions, calculations)
2. Integration tests with mock audio data
3. Stereo/mono fallback tests

---

## Dependencies

- `SileroVADNode` for VAD (optional if consuming VAD events)
- `DriftMetrics` for `TimingDriftNode`
- Existing analysis nodes for `SessionHealthNode` inputs
