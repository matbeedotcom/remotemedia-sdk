//! SRT Ingest Gateway
//!
//! Push-based SRT ingest endpoint for real-time stream health monitoring.
//! Users push media via FFmpeg/GStreamer to a private SRT URL; the gateway
//! demuxes, decodes, runs analysis pipelines, and emits alerts via webhooks + SSE.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        remotemedia-ingest-srt                           │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  ┌──────────────────┐                                                   │
//! │  │  HTTP API Server │ ← POST /api/ingest/sessions                      │
//! │  │  (axum)          │ ← GET  /api/ingest/sessions/:id/events (SSE)     │
//! │  └────────┬─────────┘                                                   │
//! │           │ creates                                                     │
//! │           ▼                                                             │
//! │  ┌──────────────────┐   ┌──────────────────┐   ┌────────────────────┐  │
//! │  │  Session Manager │──▶│  JWT Validator   │──▶│  Pipeline Registry │  │
//! │  │  (sessions map)  │   │  (jsonwebtoken)  │   │  (templates)       │  │
//! │  └────────┬─────────┘   └──────────────────┘   └────────────────────┘  │
//! │           │ spawns                                                      │
//! │           ▼                                                             │
//! │  ┌──────────────────────────────────────────────────────────────────┐  │
//! │  │  SRT Listener (port 9000)                                         │  │
//! │  └──────────────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

pub mod config;
pub mod streamid;
pub mod jwt;
pub mod session;
pub mod api;
pub mod listener;
pub mod pipeline;
pub mod webhook;
pub mod queue;
pub mod metrics;
pub mod demuxer;
