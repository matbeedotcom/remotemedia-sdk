# RemoteMedia SDK - Top-Level Makefile
# =====================================
# Build configurations for runtime-core, transports, and examples.
# Works on both Windows (PowerShell/cmd) and Unix (bash).

.DEFAULT_GOAL := help

# Platform detection
ifeq ($(OS),Windows_NT)
  SETUP_FFMPEG := powershell.exe -ExecutionPolicy Bypass -File setup-ffmpeg.ps1
else
  SETUP_FFMPEG := ./setup-ffmpeg.sh
endif

# Build profiles
RELEASE_FLAGS := --release
PROFILE ?= dev

ifeq ($(PROFILE),release)
  CARGO_FLAGS := $(RELEASE_FLAGS)
else ifeq ($(PROFILE),fast)
  CARGO_FLAGS := --profile fast
else
  CARGO_FLAGS :=
endif

# =============================================================================
# HELP
# =============================================================================

.PHONY: help
help: ## Show this help message
	@echo.
	@echo RemoteMedia SDK Build System
	@echo ==============================
	@echo.
	@echo Usage: make [target] [PROFILE=dev/release/fast]
	@echo.
	@echo Runtime Core Targets:
	@echo   core-default                   Build runtime-core with default features
	@echo   core-cuda                      Build with CUDA support (requires CUDA 12.x)
	@echo   core-minimal                   Build with no default features
	@echo   core-multiprocess              Build with only multiprocess feature
	@echo   core-silero                    Build with only silero-vad feature
	@echo   core-docker                    Build with only docker feature
	@echo   core-video                     Build with video feature (FFmpeg-based)
	@echo   core-video-pure-rust           Build with pure-Rust video codecs
	@echo   core-grpc-client               Build with gRPC client
	@echo   core-all-features              Build with all features enabled
	@echo.
	@echo Transport Targets:
	@echo   transports-all                 Build all transports
	@echo   transport-grpc                 Build gRPC transport
	@echo   transport-grpc-server          Build gRPC transport with server
	@echo   transport-http                 Build HTTP transport
	@echo   transport-http-server          Build HTTP transport with server
	@echo   transport-webrtc               Build WebRTC transport
	@echo   transport-webrtc-full          Build WebRTC with all features
	@echo   transport-ffi                  Build FFI transport (Python)
	@echo   transport-ffi-napi             Build FFI transport (Node.js)
	@echo.
	@echo Server Targets:
	@echo   servers-all                    Build all server binaries
	@echo   server-grpc                    Build gRPC server binary
	@echo   server-http                    Build HTTP server binary
	@echo   server-webrtc                  Build WebRTC server binary
	@echo.
	@echo CLI Targets:
	@echo   cli                            Build all CLI tools
	@echo   cli-remotemedia                Build main remotemedia CLI
	@echo   cli-transcribe                 Build transcribe-srt CLI
	@echo   cli-embed                      Build pipeline-embed (set PIPELINE_YAML)
	@echo.
	@echo Test Targets:
	@echo   test                           Run all tests
	@echo   test-core                      Run runtime-core tests
	@echo   test-core-unit                 Run runtime-core unit tests
	@echo   test-core-integration          Run runtime-core integration tests
	@echo   test-core-docker               Run Docker-specific tests
	@echo   test-core-vad                  Run VAD-specific tests
	@echo   test-transports                Run all transport tests
	@echo.
	@echo Benchmark Targets:
	@echo   bench                          Run all benchmarks
	@echo   bench-latency                  Run latency benchmarks
	@echo   bench-vad                      Run VAD benchmarks
	@echo   bench-docker                   Run Docker benchmarks
	@echo   bench-pipeline                 Run pipeline benchmarks
	@echo.
	@echo Utility Targets:
	@echo   check                          Check all packages compile
	@echo   clippy                         Run clippy on all packages
	@echo   fmt                            Format all Rust code
	@echo   doc                            Build documentation
	@echo   clean                          Clean build artifacts
	@echo   clean-all                      Clean everything including examples
	@echo.

# =============================================================================
# SETUP TARGETS
# =============================================================================

.PHONY: setup-ffmpeg

setup-ffmpeg: ## Download and configure FFmpeg (required for video feature)
	$(SETUP_FFMPEG)

# =============================================================================
# BUILD ALL
# =============================================================================

.PHONY: all build
all: build ## Build everything (alias for build)

build: setup-ffmpeg core-default transports-all ## Build runtime-core and all transports

build-release: ## Build everything in release mode
	$(MAKE) build PROFILE=release

# =============================================================================
# RUNTIME-CORE TARGETS
# =============================================================================

.PHONY: core-default core-minimal core-multiprocess core-silero core-docker core-video core-video-pure-rust core-grpc-client core-cuda core-all-features

core-default: setup-ffmpeg ## Build runtime-core with default features (multiprocess, silero-vad, docker, video)
	cargo build -p remotemedia-runtime-core $(CARGO_FLAGS)

core-cuda: ## Build runtime-core with CUDA support (requires CUDA 12.x toolkit)
	cargo build -p remotemedia-runtime-core --features cuda $(CARGO_FLAGS)

core-minimal: ## Build runtime-core with no default features
	cargo build -p remotemedia-runtime-core --no-default-features $(CARGO_FLAGS)

core-multiprocess: ## Build runtime-core with only multiprocess feature
	cargo build -p remotemedia-runtime-core --no-default-features --features multiprocess $(CARGO_FLAGS)

core-silero: ## Build runtime-core with only silero-vad feature
	cargo build -p remotemedia-runtime-core --no-default-features --features silero-vad $(CARGO_FLAGS)

core-docker: ## Build runtime-core with only docker feature
	cargo build -p remotemedia-runtime-core --no-default-features --features docker $(CARGO_FLAGS)

core-video: setup-ffmpeg ## Build runtime-core with video feature (FFmpeg-based)
	cargo build -p remotemedia-runtime-core --no-default-features --features video $(CARGO_FLAGS)

core-video-pure-rust: ## Build runtime-core with pure-Rust video codecs (rav1e, dav1d)
	cargo build -p remotemedia-runtime-core --no-default-features --features video-pure-rust $(CARGO_FLAGS)

core-grpc-client: ## Build runtime-core with gRPC client for RemotePipelineNode
	cargo build -p remotemedia-runtime-core --no-default-features --features grpc-client $(CARGO_FLAGS)

core-all-features: setup-ffmpeg ## Build runtime-core with all features enabled
	cargo build -p remotemedia-runtime-core --all-features $(CARGO_FLAGS)

# =============================================================================
# TRANSPORT TARGETS
# =============================================================================

.PHONY: transports-all transport-grpc transport-grpc-server transport-grpc-multiprocess transport-http transport-http-server transport-webrtc transport-webrtc-full transport-webrtc-codecs transport-webrtc-signaling transport-ffi transport-ffi-python transport-ffi-napi transport-ffi-webrtc

transports-all: transport-grpc transport-http transport-webrtc transport-ffi ## Build all transports

# --- gRPC Transport ---
transport-grpc: ## Build gRPC transport with default features
	cargo build -p remotemedia-grpc $(CARGO_FLAGS)

transport-grpc-server: ## Build gRPC transport with server binary
	cargo build -p remotemedia-grpc --features server $(CARGO_FLAGS)

transport-grpc-multiprocess: ## Build gRPC transport with multiprocess support
	cargo build -p remotemedia-grpc --features multiprocess $(CARGO_FLAGS)

# --- HTTP Transport ---
transport-http: ## Build HTTP transport with default features
	cargo build -p remotemedia-http $(CARGO_FLAGS)

transport-http-server: ## Build HTTP transport with server binary
	cargo build -p remotemedia-http --features server $(CARGO_FLAGS)

# --- WebRTC Transport ---
transport-webrtc: ## Build WebRTC transport with default features
	cargo build -p remotemedia-webrtc $(CARGO_FLAGS)

transport-webrtc-codecs: ## Build WebRTC transport with VP8/VP9 video codecs
	cargo build -p remotemedia-webrtc --features codecs $(CARGO_FLAGS)

transport-webrtc-h264: ## Build WebRTC transport with H.264 codec
	cargo build -p remotemedia-webrtc --features h264 $(CARGO_FLAGS)

transport-webrtc-signaling: ## Build WebRTC transport with gRPC signaling
	cargo build -p remotemedia-webrtc --features grpc-signaling $(CARGO_FLAGS)

transport-webrtc-ws: ## Build WebRTC transport with WebSocket signaling
	cargo build -p remotemedia-webrtc --features ws-signaling $(CARGO_FLAGS)

transport-webrtc-full: ## Build WebRTC transport with all features
	cargo build -p remotemedia-webrtc --features full $(CARGO_FLAGS)

# --- FFI Transport ---
transport-ffi: ## Build FFI transport with default features (Python)
	cargo build -p remotemedia-ffi $(CARGO_FLAGS)

transport-ffi-python: ## Build FFI transport with Python bindings
	cargo build -p remotemedia-ffi --features python,extension-module $(CARGO_FLAGS)

transport-ffi-napi: ## Build FFI transport with Node.js bindings
	cargo build -p remotemedia-ffi --no-default-features --features napi $(CARGO_FLAGS)

transport-ffi-webrtc: ## Build FFI transport with WebRTC support for Python
	cargo build -p remotemedia-ffi --features python-webrtc $(CARGO_FLAGS)

transport-ffi-napi-webrtc: ## Build FFI transport with WebRTC support for Node.js
	cargo build -p remotemedia-ffi --no-default-features --features napi-webrtc $(CARGO_FLAGS)

# =============================================================================
# SERVER BINARIES
# =============================================================================

.PHONY: server-grpc server-http server-webrtc servers-all

servers-all: server-grpc server-http server-webrtc ## Build all server binaries

server-grpc: ## Build gRPC server binary
	cargo build -p remotemedia-grpc --bin grpc-server --features server $(CARGO_FLAGS)

server-http: ## Build HTTP server binary
	cargo build -p remotemedia-http --bin http-server --features server $(CARGO_FLAGS)

server-webrtc: ## Build WebRTC server binary
	cargo build -p remotemedia-webrtc --bin webrtc_server $(CARGO_FLAGS)

# =============================================================================
# EXAMPLE/CLI TARGETS
# =============================================================================

.PHONY: cli cli-remotemedia cli-transcribe cli-embed examples-all

examples-all: cli ## Build all examples

cli: cli-remotemedia cli-transcribe ## Build all CLI tools

cli-remotemedia: ## Build the main remotemedia CLI
	cd examples && cargo build -p remotemedia-cli $(CARGO_FLAGS)

cli-transcribe: ## Build the transcribe-srt CLI tool
	cd examples && cargo build -p transcribe-srt $(CARGO_FLAGS)

cli-embed: setup-ffmpeg ## Build pipeline-embed (set PIPELINE_YAML env var)
ifndef PIPELINE_YAML
	@echo Usage: make cli-embed PIPELINE_YAML=/absolute/path/to/pipeline.yaml
	@echo        Output binary: examples/target/release/pipeline-runner
else
	cd examples && cargo build -p pipeline-embed $(CARGO_FLAGS)
endif

# =============================================================================
# TEST TARGETS
# =============================================================================

.PHONY: test test-core test-core-unit test-core-integration test-transports test-grpc test-http test-webrtc test-ffi test-cli

test: test-core test-transports ## Run all tests

test-core: ## Run all runtime-core tests
	cargo test -p remotemedia-runtime-core

test-core-unit: ## Run runtime-core unit tests only
	cargo test -p remotemedia-runtime-core --lib

test-core-integration: ## Run runtime-core integration tests
	cargo test -p remotemedia-runtime-core --test '*'

test-core-docker: ## Run Docker-specific integration tests
	cargo test -p remotemedia-runtime-core --test test_docker_multiprocess
	cargo test -p remotemedia-runtime-core --test test_docker_multiprocess_e2e
	cargo test -p remotemedia-runtime-core --test test_docker_image_builder
	cargo test -p remotemedia-runtime-core --test test_docker_resource_limits

test-core-vad: ## Run VAD-specific tests
	cargo test -p remotemedia-runtime-core --test test_speculative_vad
	cargo test -p remotemedia-runtime-core --test test_speculative_vad_coordinator

test-transports: test-grpc test-http test-webrtc test-ffi ## Run all transport tests

test-grpc: ## Run gRPC transport tests
	cargo test -p remotemedia-grpc

test-http: ## Run HTTP transport tests
	cargo test -p remotemedia-http

test-webrtc: ## Run WebRTC transport tests
	cargo test -p remotemedia-webrtc

test-ffi: ## Run FFI transport tests
	cargo test -p remotemedia-ffi

test-cli: ## Run CLI example tests
	cd examples && cargo test -p remotemedia-cli

# =============================================================================
# BENCHMARK TARGETS
# =============================================================================

.PHONY: bench bench-latency bench-vad bench-docker bench-pipeline bench-validation bench-all

bench: bench-all ## Run all benchmarks (alias)

bench-all: ## Run all benchmarks
	cargo bench -p remotemedia-runtime-core

bench-latency: ## Run latency benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_latency

bench-vad: ## Run VAD benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_real_vad_comparison

bench-docker: ## Run Docker-related benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_docker_latency
	cargo bench -p remotemedia-runtime-core --bench docker_vs_multiprocess
	cargo bench -p remotemedia-runtime-core --bench docker_e2e_pipeline

bench-pipeline: ## Run pipeline benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_real_pipeline
	cargo bench -p remotemedia-runtime-core --bench bench_utterance_completion

bench-validation: ## Run validation benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_validation

bench-speculative: ## Run speculative coordinator benchmarks
	cargo bench -p remotemedia-runtime-core --bench bench_speculative_coordinator

# =============================================================================
# CHECK & LINT TARGETS
# =============================================================================

.PHONY: check check-core check-transports check-examples clippy fmt

check: check-core check-transports check-examples ## Check all packages compile

check-core: ## Check runtime-core compiles
	cargo check -p remotemedia-runtime-core

check-transports: ## Check all transports compile
	cargo check -p remotemedia-grpc
	cargo check -p remotemedia-http
	cargo check -p remotemedia-webrtc
	cargo check -p remotemedia-ffi

check-examples: ## Check examples compile
	cd examples && cargo check --workspace

clippy: ## Run clippy on all packages
	cargo clippy --workspace -- -D warnings

fmt: ## Format all Rust code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

# =============================================================================
# DOCUMENTATION
# =============================================================================

.PHONY: doc doc-open

doc: ## Build documentation for all packages
	cargo doc --workspace --no-deps

doc-open: ## Build and open documentation
	cargo doc --workspace --no-deps --open

# =============================================================================
# CLEAN TARGETS
# =============================================================================

.PHONY: clean clean-core clean-transports clean-examples clean-all

clean: ## Clean main workspace build artifacts
	cargo clean

clean-examples: ## Clean examples workspace build artifacts
	cd examples && cargo clean

clean-all: clean clean-examples ## Clean everything
