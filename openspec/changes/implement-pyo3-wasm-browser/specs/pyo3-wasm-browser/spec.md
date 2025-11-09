# PyO3 WASM Browser Runtime Specification

## ADDED Requirements

### Requirement: PyO3 CPython WASM Compilation
The system SHALL compile the Rust runtime to wasm32-wasi target with embedded CPython 3.12 via PyO3 and static libpython3.12.a.

#### Scenario: Build WASM binary with wlr-libpy
- **GIVEN** Cargo.toml configured with wlr-libpy build dependency
- **WHEN** developer runs `cargo build --target wasm32-wasi --bin pipeline_executor_wasm`
- **THEN** build SHALL succeed and produce remotemedia_runtime.wasm binary

#### Scenario: Link static libpython in WASM
- **GIVEN** wlr-libpy with py312 feature fetches libpython3.12.a for wasm32-wasi
- **WHEN** linking WASM module
- **THEN** linker SHALL successfully embed libpython3.12.a and wasi-sysroot dependencies

#### Scenario: Initialize PyO3 in WASM context
- **GIVEN** WASM binary with embedded CPython
- **WHEN** binary starts execution
- **THEN** it SHALL call prepare_freethreaded_python() and acquire GIL successfully

### Requirement: WASI Command Entry Point
The system SHALL provide a WASI Command (_start export) that executes pipelines from manifest JSON.

#### Scenario: Read manifest from stdin
- **GIVEN** pipeline manifest as JSON string
- **WHEN** WASM binary executes with manifest passed to stdin
- **THEN** it SHALL read entire stdin into buffer and parse as JSON

#### Scenario: Write results to stdout
- **GIVEN** pipeline execution completes successfully
- **WHEN** results are serialized to JSON
- **THEN** WASM binary SHALL write JSON to stdout and exit with code 0

#### Scenario: Handle execution errors via stderr
- **GIVEN** pipeline execution fails with error
- **WHEN** error occurs
- **THEN** WASM binary SHALL write error message to stderr and exit with non-zero code

### Requirement: WASM-Compatible Data Marshaling
The system SHALL serialize numpy arrays using JSON+base64 in WASM context instead of zero-copy rust-numpy.

#### Scenario: Serialize numpy array to JSON
- **GIVEN** Python numpy array in WASM context
- **WHEN** marshaling to Rust
- **THEN** system SHALL call array.tobytes(), base64-encode, and include dtype/shape metadata

#### Scenario: Deserialize JSON to numpy array
- **GIVEN** JSON with __numpy__ marker and base64 data
- **WHEN** marshaling to Python
- **THEN** system SHALL base64-decode, call np.frombuffer(), and reshape according to metadata

#### Scenario: Conditional compilation for marshaling
- **GIVEN** codebase with both native and WASM marshaling paths
- **WHEN** compiling for native target
- **THEN** rust-numpy zero-copy path SHALL be used
- **WHEN** compiling for wasm32-wasi target
- **THEN** base64 serialization path SHALL be used

### Requirement: Synchronous Execution Path
The system SHALL provide synchronous pipeline execution for WASM using futures executor.

#### Scenario: Execute pipeline synchronously
- **GIVEN** parsed pipeline manifest
- **WHEN** executor.execute_sync() is called in WASM
- **THEN** it SHALL use futures::executor::block_on to run async executor and return results

#### Scenario: Native async execution unchanged
- **GIVEN** native runtime build
- **WHEN** executor.execute() is called
- **THEN** it SHALL use tokio async runtime normally (no regression)

### Requirement: Browser Integration via @wasmer/sdk
The system SHALL provide TypeScript integration layer for loading and executing WASM in browsers.

#### Scenario: Load .rmpkg bundle in browser
- **GIVEN** .rmpkg bundle URL
- **WHEN** PipelineRunner.initialize(url) is called
- **THEN** it SHALL fetch bundle, extract WASM module and wasi-deps, and initialize Wasmer instance

#### Scenario: Execute pipeline from JavaScript
- **GIVEN** initialized PipelineRunner and manifest JSON
- **WHEN** runner.execute(manifestJson) is called
- **THEN** it SHALL:
  1. Create WASI instance with preopen directories
  2. Pass manifest via stdin
  3. Execute WASM _start function
  4. Read results from stdout
  5. Parse JSON and return to JavaScript

#### Scenario: Handle WASM execution errors in JavaScript
- **GIVEN** WASM execution fails
- **WHEN** runner.execute() is awaited
- **THEN** it SHALL reject promise with error message from stderr

### Requirement: .rmpkg Package Format for WASM
The system SHALL extend .rmpkg format to include WASM binaries and WASI dependencies.

#### Scenario: Package WASM runtime in .rmpkg
- **GIVEN** compiled remotemedia_runtime.wasm binary
- **WHEN** creating .rmpkg bundle
- **THEN** package SHALL include:
  - modules/remotemedia_runtime.wasm
  - wasi-deps/usr/ (Python stdlib subset)
  - manifest.json with runtime_target: "wasm32-wasi"

#### Scenario: Validate WASM-specific .rmpkg structure
- **GIVEN** .rmpkg bundle with runtime_target: "wasm32-wasi"
- **WHEN** loading in browser
- **THEN** loader SHALL verify WASM binary exists and wasi-deps are present

#### Scenario: Bundle size optimization
- **GIVEN** .rmpkg bundle for WASM
- **WHEN** building package
- **THEN** build system SHOULD run wasm-opt for size/speed optimization

### Requirement: WASI Filesystem Integration
The system SHALL support WASI preopen directories for Python stdlib and model access.

#### Scenario: Map /usr directory for Python stdlib
- **GIVEN** WASM execution in browser
- **WHEN** Wasmer instance is created
- **THEN** it SHALL map /usr to wasi-deps/usr from .rmpkg bundle

#### Scenario: Access Python stdlib from WASM
- **GIVEN** Python node code imports json, sys, math modules
- **WHEN** executed in WASM
- **THEN** embedded CPython 3.12 SHALL successfully import from /usr/lib/python3.12/

#### Scenario: Load models from preopen directory (Phase 3)
- **GIVEN** Whisper node requires model file
- **WHEN** initialized in WASM
- **THEN** it SHALL access model from /models/ preopen directory

### Requirement: Browser Demo Application
The system SHALL provide reference browser demo for WASM pipeline execution.

#### Scenario: Upload and execute .rmpkg
- **GIVEN** browser demo page
- **WHEN** user uploads .rmpkg file and clicks "Execute"
- **THEN** demo SHALL load bundle, execute pipeline, and display results

#### Scenario: Display execution metrics
- **GIVEN** pipeline execution completes
- **WHEN** results are rendered
- **THEN** demo SHALL display execution time, memory usage, and output data

#### Scenario: Browser compatibility
- **GIVEN** browser demo
- **WHEN** loaded in Chrome, Firefox, or Safari (latest versions)
- **THEN** demo SHALL work without errors

### Requirement: Performance Benchmarking
The system SHALL track WASM vs native execution performance.

#### Scenario: Benchmark simple pipeline (Rust nodes)
- **GIVEN** pipeline with MultiplyNode → AddNode
- **WHEN** executed in both native and WASM runtimes
- **THEN** WASM SHALL be within 1.2-1.5x of native execution time

#### Scenario: Benchmark numpy marshaling overhead
- **GIVEN** pipeline with large numpy arrays (1MB+)
- **WHEN** marshaling in WASM (base64) vs native (zero-copy)
- **THEN** base64 path SHALL be within 2-3x of zero-copy path

#### Scenario: Benchmark cold start time
- **GIVEN** .rmpkg bundle loaded for first time
- **WHEN** measuring time from fetch to first execution
- **THEN** cold start SHALL be < 5 seconds on modern browser

## MODIFIED Requirements

### Requirement: WASM Runtime Integration (from wasm-sandbox spec)
The system SHALL integrate WASM runtime for both node execution AND full pipeline execution.

#### Scenario: Execute full pipeline in WASM (ADDED)
- **GIVEN** complete pipeline manifest
- **WHEN** loaded in WASM runtime
- **THEN** entire pipeline SHALL execute within WASM sandbox with PyO3+CPython

### Requirement: WASI Support (from wasm-sandbox spec)
The system SHALL support WASI for both server-side and browser environments.

#### Scenario: Browser WASI runtime (ADDED)
- **GIVEN** WASM binary in browser
- **WHEN** using @wasmer/sdk
- **THEN** WASI syscalls SHALL be emulated using IndexedDB and browser APIs

## Notes

### Relationship to Existing Specs
This spec extends and implements:
- `wasm-sandbox` spec: Focuses on browser-specific WASM execution with PyO3+CPython
- `pipeline-packaging` spec: Adds WASM-specific .rmpkg format requirements

### Implementation Priority
1. **Phase 1 (MVP)**: Requirements marked "ADDED" under PyO3 WASM Compilation, WASI Command, Data Marshaling, Sync Execution
2. **Phase 2 (Browser)**: Requirements under Browser Integration, .rmpkg Format, WASI Filesystem, Demo Application
3. **Phase 3 (Optional)**: Whisper WASM integration (separate spec delta)

### Dependencies
- VMware Labs `wlr-libpy` crate (external dependency)
- PyO3 0.26 with abi3-py311 feature
- @wasmer/sdk JavaScript library
- Existing Rust runtime infrastructure (executor, manifest parser, nodes)

### Testing Strategy
- **Unit tests**: Data marshaling round-trip, sync executor
- **Integration tests**: Full pipeline execution in wasmtime
- **Browser tests**: Selenium/Playwright automated browser testing
- **Performance tests**: Benchmarks comparing WASM vs native
