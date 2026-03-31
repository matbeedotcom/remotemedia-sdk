## ADDED Requirements

### Requirement: Node Provider Trait

The system SHALL provide a `NodeProvider` trait that allows external crates to register nodes with the streaming registry at compile time.

#### Scenario: Custom node crate registration
- **WHEN** a crate implements `NodeProvider` and uses `inventory::submit!`
- **AND** the crate is added as a dependency in Cargo.toml
- **THEN** the provider's nodes MUST be automatically registered when `create_default_streaming_registry()` is called

#### Scenario: Provider name identification
- **WHEN** a `NodeProvider` is registered
- **THEN** the provider MUST expose a `provider_name()` method returning a human-readable identifier

### Requirement: Compile-Time Collection

The system SHALL use the `inventory` crate to collect all `NodeProvider` implementations at compile time.

#### Scenario: Multi-crate registration
- **WHEN** multiple crates each implement `NodeProvider`
- **AND** all crates are dependencies in the final binary
- **THEN** all providers MUST be discovered and their nodes registered

#### Scenario: No provider duplication
- **WHEN** a provider is registered via `inventory::submit!`
- **THEN** each provider instance MUST be registered exactly once

### Requirement: Feature-Gated Providers

The system SHALL support feature flags to conditionally include node providers.

#### Scenario: Python nodes feature flag
- **WHEN** the `python-nodes` feature is enabled in `remotemedia-core`
- **THEN** the Python node factories (WhisperX, Kokoro TTS, etc.) MUST be available
- **AND** when the feature is disabled, the Python nodes MUST NOT be compiled

#### Scenario: Candle nodes feature flag
- **WHEN** the `candle-nodes` feature is enabled
- **THEN** the Candle ML node factories MUST be available

### Requirement: Python Nodes Separation

The system SHALL provide Python node wrapper factories in a separate `remotemedia-nodes-python` crate.

#### Scenario: Independent Python crate
- **WHEN** `remotemedia-nodes-python` is added as a dependency
- **THEN** WhisperXNode, HFWhisperNode, KokoroTTSNode, VibeVoiceTTSNode, and other Python-based nodes MUST be registered

#### Scenario: Core independence
- **WHEN** `remotemedia-nodes-python` is not a dependency
- **THEN** `remotemedia-core` MUST compile and function without Python-related code

### Requirement: Backward Compatibility

The system SHALL maintain backward compatibility with existing registry usage patterns.

#### Scenario: Existing create_default_streaming_registry usage
- **WHEN** code calls `create_default_streaming_registry()`
- **THEN** all available node factories MUST be registered as before
- **AND** no API changes MUST be required for existing consumers

#### Scenario: Manual factory registration
- **WHEN** a user manually calls `registry.register(Arc::new(MyFactory))`
- **THEN** the manual registration MUST continue to work alongside provider-based registration

### Requirement: Provider Logging

The system SHALL log provider registration for debugging purposes.

#### Scenario: Provider load logging
- **WHEN** a `NodeProvider` registers its nodes
- **THEN** the system MUST log at `debug` level with the provider name and number of nodes registered

#### Scenario: Registration failure logging
- **WHEN** a provider fails to register a node
- **THEN** the system MUST log at `warn` level with the error details
