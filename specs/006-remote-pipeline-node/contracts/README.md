# Contracts: Remote Pipeline Execution Nodes

**Feature**: 006-remote-pipeline-node

This directory contains API contract documentation for remote pipeline execution.

## Contents

- **remote-node-config-schema.md**: JSON schema for RemotePipeline node configuration
- **transport-client-api.md**: PipelineClient trait API specification
- **error-handling.md**: Error types, codes, and diagnostic messages

## Purpose

These contracts define:
1. How users configure remote pipeline nodes in manifests
2. How transport clients implement the PipelineClient interface
3. What errors can occur and how to diagnose them

## Versioning

All contracts are versioned with the runtime-core library. Breaking changes require major version bump.

Current version: v0.4.x (aligned with runtime-core)
