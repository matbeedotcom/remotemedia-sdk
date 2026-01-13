<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

## Active Technologies
- Rust 1.75+ (workspace rust-version) + tokio (async runtime), hdrhistogram (P50/P95/P99), bitflags 2.4+ (alerts), serde (serialization) (026-streaming-scheduler-migration)
- In-memory only (metrics, drift buffers); no persistence (026-streaming-scheduler-migration)

## Recent Changes
- 026-streaming-scheduler-migration: Added Rust 1.75+ (workspace rust-version) + tokio (async runtime), hdrhistogram (P50/P95/P99), bitflags 2.4+ (alerts), serde (serialization)
