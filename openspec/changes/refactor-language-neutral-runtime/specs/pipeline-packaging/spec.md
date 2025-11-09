# Pipeline Packaging Specification

## ADDED Requirements

### Requirement: OCI-Style Package Format
The system SHALL support an OCI-compatible package format (.rmpkg) for distributing pipeline artifacts.

#### Scenario: Package contains required components
- **GIVEN** a pipeline ready for packaging
- **WHEN** user runs `remotemedia build pipeline.py`
- **THEN** the system SHALL create .rmpkg containing: manifest.json, modules/, models/, and meta/

#### Scenario: Package structure validation
- **GIVEN** a .rmpkg file
- **WHEN** the runtime attempts to load it
- **THEN** it SHALL validate presence of manifest.json and reject invalid packages

#### Scenario: Package signature verification
- **GIVEN** a signed .rmpkg package
- **WHEN** the runtime loads it
- **THEN** it SHALL verify signature before execution and reject tampered packages

### Requirement: Pipeline Serialization
The system SHALL serialize Python pipeline definitions to JSON manifests.

#### Scenario: Serialize pipeline to JSON
- **GIVEN** a Python Pipeline object with nodes
- **WHEN** user calls `pipeline.serialize()`
- **THEN** it SHALL return JSON manifest with nodes, edges, and metadata

#### Scenario: Preserve node configuration
- **GIVEN** nodes with custom parameters and state
- **WHEN** pipeline is serialized
- **THEN** all node configurations SHALL be preserved in manifest

#### Scenario: Handle complex data types
- **GIVEN** nodes with numpy arrays, dataclasses, or custom objects
- **WHEN** serialized
- **THEN** system SHALL use appropriate serialization (pickle, msgpack) and reference in manifest

### Requirement: Build Command
The system SHALL provide CLI command to build pipeline packages from Python source.

#### Scenario: Build package from Python file
- **GIVEN** a Python file defining a pipeline
- **WHEN** user runs `remotemedia build pipeline.py`
- **THEN** system SHALL generate .rmpkg with unique SHA256-based name

#### Scenario: Include dependencies
- **GIVEN** a pipeline using external Python packages
- **WHEN** building package
- **THEN** system SHALL analyze imports and include dependency metadata in meta/runtime.json

#### Scenario: Optimize model weights
- **GIVEN** a pipeline using large ML models
- **WHEN** building with --optimize flag
- **THEN** system SHALL quantize/compress models and store in models/ directory

### Requirement: Registry Publishing
The system SHALL support publishing packages to OCI-compatible registries.

#### Scenario: Push package to registry
- **GIVEN** a built .rmpkg package
- **WHEN** user runs `remotemedia push oci://registry.ai/voice_tts:1.0`
- **THEN** system SHALL upload package to registry with version tag

#### Scenario: Handle authentication
- **GIVEN** a private registry requiring credentials
- **WHEN** pushing package
- **THEN** system SHALL authenticate using credentials from config or environment

#### Scenario: Tag management
- **GIVEN** a package already exists with tag :latest
- **WHEN** pushing new version with same tag
- **THEN** system SHALL update tag to point to new package

### Requirement: Package Caching
The system SHALL cache downloaded packages locally for offline usage and performance.

#### Scenario: Cache on first download
- **GIVEN** a remote package reference oci://registry.ai/model:1.0
- **WHEN** runtime fetches it for first time
- **THEN** it SHALL cache package in ~/.remotemedia/pipelines/<sha256>/

#### Scenario: Use cached package
- **GIVEN** a previously cached package
- **WHEN** runtime encounters same reference again
- **THEN** it SHALL use cached version without network request

#### Scenario: Cache invalidation
- **GIVEN** a cached package
- **WHEN** user runs `remotemedia cache clear` or package signature changes
- **THEN** system SHALL remove cached version and re-fetch on next use

### Requirement: Automatic Package Retrieval
The system SHALL automatically fetch missing packages when referenced in pipelines.

#### Scenario: Fetch on reference
- **GIVEN** a HFPipelineNode with remote_ref="oci://registry.ai/tts:1.0"
- **WHEN** pipeline runs
- **THEN** runtime SHALL automatically fetch package if not cached

#### Scenario: Parallel package fetching
- **GIVEN** a pipeline with multiple remote package references
- **WHEN** pipeline initializes
- **THEN** runtime SHALL fetch all packages in parallel

#### Scenario: Handle fetch failures gracefully
- **GIVEN** a package reference that cannot be fetched (network error)
- **WHEN** runtime attempts fetch
- **THEN** it SHALL return clear error and suggest fallback or local execution

### Requirement: Manifest Validation
The system SHALL validate manifest structure and content before execution.

#### Scenario: Validate required fields
- **GIVEN** a manifest missing "nodes" field
- **WHEN** runtime loads it
- **THEN** it SHALL reject with error listing required fields

#### Scenario: Validate node type compatibility
- **GIVEN** a manifest with unsupported node type
- **WHEN** runtime parses it
- **THEN** it SHALL return error indicating supported types

#### Scenario: Check version compatibility
- **GIVEN** a manifest with version "2.0" but runtime supports "1.x"
- **WHEN** loading manifest
- **THEN** runtime SHALL reject with version mismatch error

### Requirement: Peer-to-Peer Package Transfer
The system SHALL support direct package transfer between peers via WebRTC.

#### Scenario: Transfer package over WebRTC data channel
- **GIVEN** two peers connected via WebRTC
- **WHEN** one peer references a package the other has cached
- **THEN** system SHALL transfer package over data channel without registry

#### Scenario: Resume interrupted transfers
- **GIVEN** a package transfer in progress
- **WHEN** connection drops temporarily
- **THEN** system SHALL resume transfer from last chunk when reconnected

#### Scenario: Verify transferred package integrity
- **GIVEN** a package received via P2P transfer
- **WHEN** transfer completes
- **THEN** receiver SHALL verify SHA256 hash matches manifest

### Requirement: Package Provenance
The system SHALL track package origin, build metadata, and signing information.

#### Scenario: Record build metadata
- **GIVEN** a package being built
- **WHEN** build completes
- **THEN** meta/provenance.json SHALL contain: build timestamp, builder identity, source hash, dependencies

#### Scenario: Signature verification chain
- **GIVEN** a signed package
- **WHEN** runtime verifies signature
- **THEN** it SHALL check signing key against trusted keyring and validate certificate chain

#### Scenario: Display package info
- **GIVEN** a cached package
- **WHEN** user runs `remotemedia inspect <package-ref>`
- **THEN** system SHALL display provenance, signature status, and metadata
