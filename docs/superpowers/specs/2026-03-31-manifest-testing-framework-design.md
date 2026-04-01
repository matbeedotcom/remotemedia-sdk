# Universal Manifest Testing Framework

**Date**: 2026-03-31
**Status**: Proposed

## Problem

The RemoteMedia SDK has extensive per-transport and per-component tests, but no unified way to validate that an arbitrary pipeline manifest works end-to-end. When a user authors a manifest (YAML/JSON), there is no single command to verify it will function correctly across all applicable execution modes (streaming, unary, WebRTC, gRPC, HTTP). Errors from Python nodes, Rust nodes, ML model loading, IPC channels, and transport layers are only discoverable by running the full system manually.

## Goal

A universal manifest testing framework that:
1. Takes any pipeline manifest and validates it works end-to-end
2. Auto-detects applicable transports and execution modes from manifest contents
3. Generates synthetic test data (including TTS-generated speech for voice pipelines)
4. Produces a structured error report covering every node and transport
5. Works as both a CLI tool (ad-hoc testing) and a cargo test harness (CI)

## Architecture

Three-layer design with a `TestPlan` intermediate representation:

```
┌─────────────────────────────────────────────────────────┐
│  remotemedia-cli test-manifest subcommand               │
│    remotemedia test-manifest ./pipeline.yaml            │
│    --transport direct|grpc|webrtc|http|all              │
│    --timeout 30  --output-format json|text|junit        │
│    --dry-run  --skip-ml  --verbose                      │
└────────────────┬────────────────────────────────────────┘
                 │
    ┌────────────▼──────────────┐
    │  manifest-analyzer        │  Pure analysis, zero execution
    │  crates/libs/             │
    │                           │
    │  Manifest → AnalysisResult│
    └────────────┬──────────────┘
                 │
    ┌────────────▼──────────────┐
    │  manifest-tester          │  Execution engine
    │  crates/libs/             │
    │                           │
    │  AnalysisResult → TestPlan│
    │  TestPlan → ProbeBackend  │──→ DirectProbe
    │            executes via   │──→ GrpcProbe   (feature-gated)
    │                           │──→ WebRtcProbe (feature-gated)
    │                           │──→ HttpProbe   (feature-gated)
    │                           │
    │  → ManifestTestReport     │
    └───────────────────────────┘
```

## Component Design

### 1. Manifest Analyzer (`crates/libs/manifest-analyzer/`)

Pure analysis crate with zero execution dependencies. Parses a manifest and produces an `AnalysisResult`.

**Pipeline classification** — walks the DAG from source nodes (no incoming connections) and classifies based on node types:

| Node Pattern | Pipeline Type | Input Type |
|---|---|---|
| AudioChunker/Resampler → VAD → Whisper | SpeechToText | Audio |
| Text → KokoroTTS/LFM2Audio | TextToSpeech | Text |
| VAD → Whisper → LLM → TTS | VoiceAssistant | Audio (speech) |
| VideoFlip/YOLO | VideoProcessing | Video |
| Mixed or unknown | Mixed/Unknown | Inferred from sources |

**Transport detection**:
- Any `is_streaming: true` node → streaming required → gRPC streaming, WebRTC applicable
- All nodes non-streaming → unary HTTP, gRPC unary applicable
- Audio/video data types → WebRTC applicable
- DirectProbe always applicable

**ML requirements mapping** — maps known `node_type` strings to dependencies:
- `SileroVADNode` → Silero ONNX model
- `WhisperXNode` / `HFWhisperNode` → Python + Whisper model + GPU preferred
- `KokoroTTSNode` → Python + Kokoro model
- `LFM2AudioNode` → Python + LFM2 model + GPU required

**Input type inference** — the analyzer uses a static lookup table mapping `node_type` strings to expected input/output `RuntimeDataType` values (from `remotemedia_core::nodes::schema::RuntimeDataType`). This keeps the analyzer free of execution dependencies. New node types require updating the table. The table covers all registered node types from the Python and Rust registries.

**Key types**:

```rust
struct AnalysisResult {
    pipeline_type: PipelineType,
    source_input_types: Vec<RuntimeDataType>,  // from remotemedia_core::nodes::schema
    sink_output_types: Vec<RuntimeDataType>,
    execution_mode: ExecutionMode,  // Unary | Streaming
    applicable_transports: Vec<TransportType>,
    ml_requirements: Vec<MlRequirement>,
    node_graph: PipelineGraph,  // Reuses crates/core/src/executor/mod.rs PipelineGraph
}

enum PipelineType {
    SpeechToText,
    TextToSpeech,
    VoiceAssistant,
    AudioProcessing,
    VideoProcessing,
    Mixed,
    Unknown,
}

struct MlRequirement {
    node_id: String,
    node_type: String,
    model_name: Option<String>,
    requires_gpu: bool,
    requires_python: bool,
}
```

### 2. Manifest Tester (`crates/libs/manifest-tester/`)

Execution engine that takes an `AnalysisResult`, builds a `TestPlan`, runs probes, and produces a report.

#### TestPlan

```rust
struct TestPlan {
    manifest: Arc<Manifest>,
    analysis: AnalysisResult,
    probes: Vec<ProbeSpec>,       // Which backends to test (see below)
    synthetic_data: SyntheticData,
    prerequisites: PrerequisiteCheck,
    timeout: Duration,
}
```

```rust
enum ProbeSpec {
    Direct,
    Grpc { port: Option<u16> },
    WebRtc { signal_port: Option<u16> },
    Http { port: Option<u16> },
}
```

The `TestPlan` enables:
- **Dry-run**: Show what would be tested without executing
- **Prerequisite filtering**: Skip probes where requirements aren't met
- **Caching**: Generate synthetic data once, reuse across probes

#### Synthetic Data Generation

For each `RuntimeDataType` the pipeline expects as input:

- **Audio**: Sine wave bursts at configurable sample rate/channels
- **Audio (speech)**: TTS-generated speech for voice pipelines (see below)
- **Text**: Canned strings ("The quick brown fox jumps over the lazy dog.")
- **Video**: Solid color frames or test patterns (port from WebRTC harness `media.rs`)
- **JSON/Binary**: Minimal valid payloads
- **Tensor/Numpy**: Zero-filled arrays with correct shape/dtype
- **File**: Temp file with known content and appropriate MIME type

Note: `ControlMessage` is excluded from synthetic input generation — it is an internal flow-control type injected by the runtime, not user-supplied input.

**Speech generation strategy for voice pipelines**:

1. **Primary**: If KokoroTTS is available locally, run a one-shot TTS pipeline to generate realistic speech from sample text. This produces audio that exercises VAD thresholds, ASR models, and downstream processing authentically.
2. **Fallback**: Generate speech-like audio — white noise bursts shaped with a speech-like amplitude envelope (alternating 300ms burst + 200ms silence). This may or may not trigger Silero VAD depending on model sensitivity, but validates pipeline data flow regardless.
3. **Cache**: Store generated audio in a temp directory, reuse across probes within the same test run.

#### Prerequisite Checking

Runs before execution to avoid wasting time on doomed tests:

```rust
struct PrerequisiteCheck {
    python_available: bool,
    python_version: Option<String>,
    gpu_available: bool,
    gpu_type: Option<GpuType>,  // CUDA, ROCm, Metal
    available_models: Vec<String>,
    missing_packages: Vec<String>,
}
```

Missing prerequisites produce `Skipped` results with actionable messages, not failures.

#### Probe Backends

```rust
#[async_trait]
trait ProbeBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn probe(&self, plan: &TestPlan) -> ProbeResult;
}
```

**DirectProbe** (always available):
- Creates `PipelineExecutor` with the node registry from `create_default_streaming_registry()`
- Streaming mode: `create_session()` → `send()` synthetic data chunks → `recv()` outputs → `close()`
- Unary mode: `execute_unary()` with synthetic data → verify output produced
- Captures per-node initialization time, processing time, and errors
- Follows pattern from `PipelineSession` in `crates/libs/pipeline-runner/src/session.rs`

**GrpcProbe** (feature `probe-grpc`):
- Starts gRPC server on `127.0.0.1:0` (ephemeral port)
- Creates gRPC client, opens streaming or unary RPC
- Sends synthetic data, collects responses with timing
- Follows pattern from `crates/transports/grpc/tests/grpc_streaming_e2e.rs`

**WebRtcProbe** (feature `probe-webrtc`):
- Starts WebRTC signaling server on random port
- Creates peer connection with loopback ICE config
- Sends audio/video tracks, receives output tracks
- Reuses `TestServer`/`TestClient` patterns from `crates/transports/webrtc/tests/harness/`

**HttpProbe** (feature `probe-http`):
- Starts HTTP server on random port
- POST manifest + data payload, receive response
- Only for unary execution mode; skipped for streaming-only pipelines

**Server lifecycle**: Each probe manages its own server. Startup includes health check polling with configurable timeout. Cleanup via `Drop` ensures no leaked processes or ports.

**Probe concurrency**: Probes run sequentially by default. GPU-bound ML nodes cannot safely share the GPU across concurrent probe sessions. A future `--parallel` flag may enable concurrent probes for environments with guaranteed resource isolation.

**`--skip-ml` behavior**: When `--skip-ml` is set, nodes identified as requiring ML models are replaced with passthrough stubs that forward their input unchanged. This preserves pipeline topology and tests data flow without requiring model downloads.

### 3. Structured Error Report

```rust
struct ManifestTestReport {
    manifest_path: PathBuf,
    manifest_name: String,
    overall_status: TestStatus,  // Pass | Fail | Partial | Skipped

    // Per-node results
    node_results: Vec<NodeResult>,

    // Per-transport results
    probe_results: Vec<ProbeResult>,

    // Aggregated latency metrics
    latency: LatencyMetrics,

    // All errors, categorized
    errors: Vec<CategorizedError>,

    duration: Duration,
}

struct NodeResult {
    node_id: String,
    node_type: String,
    status: NodeStatus,  // Initialized | Running | OutputProduced | Failed | Skipped
    init_time_ms: Option<u64>,
    process_time_ms: Option<u64>,
    error: Option<String>,
}

struct ProbeResult {
    transport: String,  // "direct", "grpc", "webrtc", "http"
    status: TestStatus,
    latency_ms: Option<u64>,
    first_output_ms: Option<u64>,  // Time to first output
    errors: Vec<CategorizedError>,
}

struct CategorizedError {
    category: ErrorCategory,
    node_id: Option<String>,
    message: String,
    source: Option<String>,  // Python traceback, Rust backtrace, etc.
}

enum ErrorCategory {
    ManifestParse,       // YAML/JSON parse failures
    ManifestValidation,  // Schema, graph, capability errors
    Prerequisite,        // Missing Python, models, GPU
    NodeInit,            // Process spawn, model load failures
    NodeExecution,       // Runtime processing errors
    Transport,           // Connection, protocol errors
    Timeout,             // Exceeded deadline
    Ipc,                 // iceoryx2 channel failures
}
```

**Output formats**:
- **Text** (default): Human-readable table with pass/fail per node and transport, color-coded
- **JSON**: Machine-readable for CI dashboards and tooling
- **JUnit XML**: For CI systems that consume JUnit format

**Manifest format**: Both YAML and JSON are supported. Format is detected by file extension (`.yaml`/`.yml` → YAML, `.json` → JSON). Both are deserialized into the same `Manifest` struct via serde.

### 4. CLI Interface (subcommand on `remotemedia-cli`)

```
remotemedia test-manifest <MANIFEST_PATH>
    --transport <direct|grpc|webrtc|http|all>  [default: direct]
    --timeout <seconds>                         [default: 30]
    --output-format <json|text|junit>           [default: text]
    --dry-run                                   # Show test plan without executing
    --skip-ml                                   # Skip nodes requiring ML models
    --tts-text <text>                           # Custom text for TTS speech generation
    --verbose                                   # Detailed per-node logging
```

Exit codes:
- `0`: All probes passed
- `1`: One or more probes failed
- `2`: Manifest parse/validation error (never reached execution)
- `3`: All probes skipped (prerequisites not met)

### 5. Cargo Test Integration

```rust
// crates/libs/manifest-tester/tests/manifest_e2e.rs

#[tokio::test]
async fn test_manifest_vad_s2s_pipeline() {
    let report = ManifestTester::test(
        "examples/nextjs-tts-app/pipelines/vad-s2s-pipeline.json"
    )
    .with_probes(&[ProbeSpec::Direct])
    .with_timeout(Duration::from_secs(30))
    .run()
    .await;

    assert!(report.overall_status == TestStatus::Pass, "{report}");
}

// ML-dependent tests gated behind #[ignore]
#[tokio::test]
#[ignore = "requires Whisper model"]
async fn test_manifest_whisper_pipeline() {
    // ...
}
```

Auto-discovery of manifests from `examples/` directories for comprehensive CI coverage.

## Key Files to Modify/Create

### New crates
- `crates/libs/manifest-analyzer/` — new workspace member
- `crates/libs/manifest-tester/` — new workspace member

### Modified crates
- `examples/cli/remotemedia-cli/` — add `test-manifest` subcommand

### Existing code to reuse
- `crates/core/src/manifest.rs` — Manifest parsing and validation
- `crates/core/src/transport/executor.rs` — `PipelineExecutor` for DirectProbe
- `crates/libs/pipeline-runner/src/session.rs` — `PipelineSession` wrapping pattern
- `crates/transports/webrtc/tests/harness/media.rs` — `MediaGenerator` for synthetic audio/video
- `crates/transports/webrtc/tests/harness/validator.rs` — `OutputValidator` patterns
- `crates/transports/grpc/tests/grpc_streaming_e2e.rs` — gRPC test patterns
- `crates/core/src/nodes/streaming_registry.rs` — Node registry for factory creation

### Workspace config
- `Cargo.toml` (root) — add new workspace members

## Verification

### Acceptance criteria

1. **Analyzer**: `ManifestAnalyzer::analyze()` on `examples/nextjs-tts-app/pipelines/vad-s2s-pipeline.json` returns `PipelineType::VoiceAssistant`, `ExecutionMode::Streaming`, and `source_input_types: [Audio]`
2. **DirectProbe**: Running DirectProbe on a simple Rust-only pipeline (e.g., PassThrough or AudioResample) produces a `Pass` result with at least one output chunk
3. **Dry-run**: `remotemedia test-manifest --dry-run` on all manifests in `examples/` completes without errors and prints a valid test plan
4. **Error reporting**: Running against an intentionally broken manifest (missing node type, invalid connection) produces categorized errors with correct `ErrorCategory`
5. **Skip-ML**: `--skip-ml` on an ML-dependent pipeline produces `Pass` with passthrough stubs (data flows through without model execution)

### Test plan

1. **Unit tests**: Each crate has its own test suite
   - Analyzer: test classification of known manifests, edge cases (empty connections, single node)
   - Tester: test probe lifecycle, synthetic data generation, report formatting
2. **Integration tests**: Run against example manifests in `examples/`
3. **CLI smoke test**: `remotemedia test-manifest examples/nextjs-tts-app/pipelines/vad-s2s-pipeline.json --dry-run`
4. **CI integration**: Add `make test-manifests` target to Makefile
