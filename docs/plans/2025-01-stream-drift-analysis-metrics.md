# Stream Drift Analysis Metrics - Implementation Plan

**Date**: 2025-01  
**Status**: Planned  
**Author**: Claude  
**Estimated Effort**: 3-4 days

## Overview

Add stream health monitoring capabilities to the RemoteMedia SDK by integrating drift analysis into the existing metrics infrastructure. This enables automatic detection of:

- **Lead/Drift**: Buffer growth and latency creep
- **Cadence variance**: Irregular frame timing
- **A/V Skew**: Audio-video synchronization drift
- **Freeze detection**: Stalled content
- **Health scoring**: Weighted aggregate quality metric

## Goals

1. **Automatic measurement** - Every node execution automatically captures timing metrics
2. **Low overhead** - Target <1μs per sample (matching existing latency metrics)
3. **Observable** - Prometheus/Grafana compatible export
4. **Alertable** - Threshold-based alerts for anomaly detection
5. **Non-breaking** - Backward compatible with existing pipelines

## Non-Goals

- Dedicated analysis nodes (DriftNode, SkewNode, etc.) - metrics layer handles this
- Real-time visualization UI (separate concern)
- SRT/RTMP transport implementation (separate feature)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Pipeline Execution                                              │
│                                                                  │
│  [Demux] → [Node A] → [Node B] → [Node C] → [Sink]             │
│     │          │          │          │                          │
│     │    ┌─────┴──────────┴──────────┴─────┐                   │
│     │    │     Metrics Collection Layer     │                   │
│     │    │  (automatic on each execution)   │                   │
│     │    └─────────────────────────────────┘                   │
│     │                    │                                       │
│     ▼                    ▼                                       │
│  arrival_ts_us      DriftMetrics                                │
│  media_ts_us          • lead_current                            │
│                       • lead_slope                              │
│                       • cadence_histogram                       │
│                       • av_skew                                 │
│                       • freeze_duration                         │
│                       • health_score()                          │
│                              │                                   │
│                              ▼                                   │
│                     Prometheus Export                           │
│                     /metrics endpoint                           │
└─────────────────────────────────────────────────────────────────┘
```

---

## Prerequisites

### Phase 0: RuntimeData Timestamp Fields

Before implementing drift metrics, RuntimeData must carry timing information.

#### 0.1 Add fields to RuntimeData::Audio

**File**: `runtime-core/src/lib.rs`

```rust
/// Audio samples (f32 PCM)
Audio {
    /// Audio samples as f32
    samples: Vec<f32>,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Number of channels (1=mono, 2=stereo)
    channels: u32,
    /// Optional stream identifier for multi-track routing (spec 013)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stream_id: Option<String>,
    
    // === NEW FIELDS ===
    
    /// Presentation timestamp in microseconds (derived from sample cursor)
    /// None = not tracked (legacy data)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp_us: Option<u64>,
    
    /// Arrival timestamp in microseconds (monotonic, relative to stream start)
    /// Captured at demux/transport ingress
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arrival_ts_us: Option<u64>,
},
```

#### 0.2 Add arrival_ts_us to RuntimeData::Video

**File**: `runtime-core/src/lib.rs`

```rust
/// Video frame
Video {
    // ... existing fields ...
    
    /// Presentation timestamp in microseconds (already exists)
    timestamp_us: u64,
    
    // === NEW FIELD ===
    
    /// Arrival timestamp in microseconds (monotonic, relative to stream start)
    /// Captured at demux/transport ingress
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arrival_ts_us: Option<u64>,
    
    // ... rest of fields ...
},
```

#### 0.3 Helper methods on RuntimeData

**File**: `runtime-core/src/lib.rs` (in `impl RuntimeData`)

```rust
impl RuntimeData {
    /// Extract timing information for drift analysis
    /// Returns (media_timestamp_us, arrival_timestamp_us)
    pub fn timing(&self) -> (Option<u64>, Option<u64>) {
        match self {
            RuntimeData::Audio { timestamp_us, arrival_ts_us, .. } => {
                (*timestamp_us, *arrival_ts_us)
            }
            RuntimeData::Video { timestamp_us, arrival_ts_us, .. } => {
                (Some(*timestamp_us), *arrival_ts_us)
            }
            _ => (None, None),
        }
    }
    
    /// Get stream identifier for per-track metrics
    pub fn stream_id(&self) -> Option<&str> {
        match self {
            RuntimeData::Audio { stream_id, .. } => stream_id.as_deref(),
            RuntimeData::Video { stream_id, .. } => stream_id.as_deref(),
            _ => None,
        }
    }
    
    /// Check if this is audio data
    pub fn is_audio(&self) -> bool {
        matches!(self, RuntimeData::Audio { .. })
    }
    
    /// Check if this is video data
    pub fn is_video(&self) -> bool {
        matches!(self, RuntimeData::Video { .. })
    }
}
```

#### 0.4 Audio timestamp derivation utility

**File**: `runtime-core/src/data/audio_timing.rs` (new file)

```rust
//! Audio timestamp derivation from sample cursor
//!
//! Maintains per-track state to derive presentation timestamps
//! from cumulative sample count.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Per-track audio timing state
#[derive(Debug, Default)]
pub struct AudioTimingTracker {
    /// Track state by stream_id (None key = default track)
    tracks: Arc<RwLock<HashMap<Option<String>, TrackState>>>,
    
    /// Monotonic base time (set on first sample)
    t0: Option<std::time::Instant>,
}

#[derive(Debug, Default)]
struct TrackState {
    /// Cumulative sample count
    sample_cursor: u64,
}

impl AudioTimingTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Derive timestamp for audio samples
    /// 
    /// # Arguments
    /// * `stream_id` - Optional stream identifier
    /// * `sample_count` - Number of samples in this chunk
    /// * `sample_rate` - Sample rate in Hz
    /// 
    /// # Returns
    /// (media_timestamp_us, arrival_timestamp_us)
    pub fn derive_timestamps(
        &self,
        stream_id: Option<&str>,
        sample_count: usize,
        sample_rate: u32,
    ) -> (u64, u64) {
        // Initialize t0 on first call
        let t0 = *self.t0.get_or_insert_with(std::time::Instant::now);
        
        // Capture arrival time
        let arrival_ts_us = t0.elapsed().as_micros() as u64;
        
        // Get or create track state
        let mut tracks = self.tracks.write().unwrap();
        let track = tracks
            .entry(stream_id.map(String::from))
            .or_default();
        
        // Derive media timestamp from sample cursor
        // media_ts_us = (sample_cursor * 1_000_000) / sample_rate
        let media_ts_us = (track.sample_cursor * 1_000_000) / sample_rate as u64;
        
        // Advance cursor
        track.sample_cursor += sample_count as u64;
        
        (media_ts_us, arrival_ts_us)
    }
    
    /// Reset timing state (e.g., on stream restart)
    pub fn reset(&mut self) {
        self.tracks.write().unwrap().clear();
        self.t0 = None;
    }
}
```

#### 0.5 Update IPC serialization

**File**: `runtime-core/src/python/multiprocess/data_transfer.rs`

Update the binary serialization format to include optional timestamp fields:

```rust
// Audio format (extended):
// type (1) | session_len (2) | session_id | timestamp (8) | 
// NEW: media_ts_us (8) | arrival_ts_us (8) | has_timing (1) |
// payload_len (4) | payload

// For backward compat, use a version byte or optional trailer
```

---

## Implementation

### Phase 1: DriftMetrics Core

#### 1.1 Create drift_metrics.rs

**File**: `runtime-core/src/executor/drift_metrics.rs`

```rust
//! Stream drift analysis metrics
//!
//! Tracks lead/drift, cadence, A/V skew, and freeze detection
//! for real-time stream health monitoring.

use hdrhistogram::Histogram;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Sample for lead slope calculation
#[derive(Clone, Copy)]
struct LeadSample {
    /// Arrival timestamp (microseconds)
    arrival_ts_us: u64,
    /// Lead value at this point (T - S) in microseconds
    lead_us: i64,
}

/// Alert flags (bitfield)
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DriftAlerts: u64 {
        /// Lead slope exceeds threshold (buffer growing)
        const DRIFT_SLOPE = 1 << 0;
        /// Lead jumped suddenly (stall then flush)
        const LEAD_JUMP = 1 << 1;
        /// A/V skew exceeds threshold
        const AV_SKEW = 1 << 2;
        /// Content freeze detected
        const FREEZE = 1 << 3;
        /// Cadence variance too high
        const CADENCE_UNSTABLE = 1 << 4;
        /// Health score below threshold
        const HEALTH_LOW = 1 << 5;
    }
}

/// Thresholds for drift alerts
#[derive(Debug, Clone)]
pub struct DriftThresholds {
    /// Lead slope threshold (μs/s) - alert if exceeded
    /// Default: 5000 (5ms per second of drift)
    pub lead_slope_alert_us_per_s: i64,
    
    /// Lead jump threshold (μs) - alert on sudden step
    /// Default: 250_000 (250ms jump)
    pub lead_jump_alert_us: i64,
    
    /// A/V skew threshold (μs)
    /// Default: 80_000 (80ms)
    pub av_skew_alert_us: i64,
    
    /// Freeze duration threshold (μs)
    /// Default: 500_000 (500ms)
    pub freeze_alert_us: u64,
    
    /// Cadence coefficient of variation threshold
    /// Default: 0.2 (20% variance)
    pub cadence_cv_alert: f64,
    
    /// Health score threshold (0.0-1.0)
    /// Default: 0.7 (70%)
    pub health_score_alert: f64,
}

impl Default for DriftThresholds {
    fn default() -> Self {
        Self {
            lead_slope_alert_us_per_s: 5_000,
            lead_jump_alert_us: 250_000,
            av_skew_alert_us: 80_000,
            freeze_alert_us: 500_000,
            cadence_cv_alert: 0.2,
            health_score_alert: 0.7,
        }
    }
}

/// Per-stream drift analysis metrics
pub struct DriftMetrics {
    /// Stream identifier
    pub stream_id: String,
    
    // === Lead/Drift tracking ===
    
    /// First arrival time (microseconds, monotonic)
    t0_arrival_us: Mutex<Option<u64>>,
    
    /// First media timestamp (microseconds)
    s0_media_us: Mutex<Option<u64>>,
    
    /// Lead history for slope calculation (circular buffer)
    lead_history: Mutex<VecDeque<LeadSample>>,
    
    /// Maximum lead history size
    lead_history_max: usize,
    
    /// Current lead value (T - S) in microseconds
    /// Positive = arrival ahead of media time (buffering)
    /// Negative = arrival behind media time (underrun risk)
    pub lead_current_us: AtomicI64,
    
    /// Previous lead value (for jump detection)
    lead_previous_us: AtomicI64,
    
    /// Lead slope (microseconds per second) - EMA smoothed
    pub lead_slope_us_per_s: AtomicI64,
    
    // === Cadence tracking ===
    
    /// Last media timestamp seen (microseconds)
    last_media_ts_us: AtomicU64,
    
    /// Cadence histogram (Δpts distribution in microseconds)
    cadence_histogram: Arc<Mutex<Histogram<u64>>>,
    
    /// Expected cadence (microseconds) - 0 = auto-detect
    expected_cadence_us: AtomicU64,
    
    /// Cadence sample count (for variance calculation)
    cadence_count: AtomicU64,
    
    /// Cadence sum (for mean calculation)
    cadence_sum_us: AtomicU64,
    
    /// Cadence sum of squares (for variance calculation)
    cadence_sum_sq: AtomicU64,
    
    // === A/V Skew tracking ===
    
    /// Last video media timestamp (microseconds)
    last_video_ts_us: AtomicU64,
    
    /// Last audio media timestamp (microseconds)
    last_audio_ts_us: AtomicU64,
    
    /// Current A/V skew in microseconds (video - audio)
    /// Positive = video ahead, Negative = audio ahead
    pub av_skew_us: AtomicI64,
    
    // === Freeze detection ===
    
    /// Last content hash (for video freeze detection)
    /// Uses simple perceptual hash (dHash)
    last_content_hash: AtomicU64,
    
    /// Freeze start timestamp (0 = not frozen)
    freeze_start_us: AtomicU64,
    
    /// Total freeze duration in current window (microseconds)
    pub total_freeze_duration_us: AtomicU64,
    
    /// Consecutive frozen frame count
    frozen_frame_count: AtomicU64,
    
    // === Alerts ===
    
    /// Alert thresholds
    thresholds: DriftThresholds,
    
    /// Currently active alerts (bitfield)
    pub active_alerts: AtomicU64,
    
    // === Statistics ===
    
    /// Total samples processed
    pub total_samples: AtomicU64,
    
    /// Last update timestamp
    pub last_update_us: AtomicU64,
}

impl DriftMetrics {
    /// Create new drift metrics for a stream
    pub fn new(stream_id: impl Into<String>) -> Result<Self, String> {
        // Cadence histogram: 1μs to 1 second range
        let cadence_hist = Histogram::<u64>::new_with_max(1_000_000, 3)
            .map_err(|e| format!("Failed to create cadence histogram: {}", e))?;
        
        Ok(Self {
            stream_id: stream_id.into(),
            t0_arrival_us: Mutex::new(None),
            s0_media_us: Mutex::new(None),
            lead_history: Mutex::new(VecDeque::with_capacity(100)),
            lead_history_max: 100,
            lead_current_us: AtomicI64::new(0),
            lead_previous_us: AtomicI64::new(0),
            lead_slope_us_per_s: AtomicI64::new(0),
            last_media_ts_us: AtomicU64::new(0),
            cadence_histogram: Arc::new(Mutex::new(cadence_hist)),
            expected_cadence_us: AtomicU64::new(0),
            cadence_count: AtomicU64::new(0),
            cadence_sum_us: AtomicU64::new(0),
            cadence_sum_sq: AtomicU64::new(0),
            last_video_ts_us: AtomicU64::new(0),
            last_audio_ts_us: AtomicU64::new(0),
            av_skew_us: AtomicI64::new(0),
            last_content_hash: AtomicU64::new(0),
            freeze_start_us: AtomicU64::new(0),
            total_freeze_duration_us: AtomicU64::new(0),
            frozen_frame_count: AtomicU64::new(0),
            thresholds: DriftThresholds::default(),
            active_alerts: AtomicU64::new(0),
            total_samples: AtomicU64::new(0),
            last_update_us: AtomicU64::new(0),
        })
    }
    
    /// Create with custom thresholds
    pub fn with_thresholds(stream_id: impl Into<String>, thresholds: DriftThresholds) -> Result<Self, String> {
        let mut metrics = Self::new(stream_id)?;
        metrics.thresholds = thresholds;
        Ok(metrics)
    }
    
    /// Record an arrival and update drift metrics
    /// 
    /// # Arguments
    /// * `media_ts_us` - Presentation timestamp from stream (None = not available)
    /// * `arrival_ts_us` - Arrival timestamp (None = not available)
    /// * `is_video` - True if video frame, false if audio
    /// * `content_hash` - Optional content hash for freeze detection (video only)
    pub fn record_arrival(
        &self,
        media_ts_us: Option<u64>,
        arrival_ts_us: Option<u64>,
        is_video: bool,
        content_hash: Option<u64>,
    ) {
        self.total_samples.fetch_add(1, Ordering::Relaxed);
        
        let Some(media_ts) = media_ts_us else { return };
        let Some(arrival_ts) = arrival_ts_us else { return };
        
        self.last_update_us.store(arrival_ts, Ordering::Relaxed);
        
        // Initialize baselines on first sample
        {
            let mut t0 = self.t0_arrival_us.lock().unwrap();
            let mut s0 = self.s0_media_us.lock().unwrap();
            
            if t0.is_none() {
                *t0 = Some(arrival_ts);
                *s0 = Some(media_ts);
                self.last_media_ts_us.store(media_ts, Ordering::Relaxed);
                return;
            }
        }
        
        // Calculate lead
        let t0 = self.t0_arrival_us.lock().unwrap().unwrap();
        let s0 = self.s0_media_us.lock().unwrap().unwrap();
        
        let t_elapsed = arrival_ts.saturating_sub(t0);
        let s_elapsed = media_ts.saturating_sub(s0);
        let lead = t_elapsed as i64 - s_elapsed as i64;
        
        // Store previous lead for jump detection
        let prev_lead = self.lead_current_us.swap(lead, Ordering::Relaxed);
        self.lead_previous_us.store(prev_lead, Ordering::Relaxed);
        
        // Update lead history
        {
            let mut history = self.lead_history.lock().unwrap();
            history.push_back(LeadSample { arrival_ts_us: arrival_ts, lead_us: lead });
            while history.len() > self.lead_history_max {
                history.pop_front();
            }
        }
        
        // Calculate lead slope
        self.update_lead_slope();
        
        // Update cadence
        let last_ts = self.last_media_ts_us.swap(media_ts, Ordering::Relaxed);
        if last_ts > 0 && media_ts > last_ts {
            let cadence = media_ts - last_ts;
            self.record_cadence(cadence);
        }
        
        // Update A/V tracking
        if is_video {
            self.last_video_ts_us.store(media_ts, Ordering::Relaxed);
            
            // Freeze detection
            if let Some(hash) = content_hash {
                self.check_freeze(hash, arrival_ts);
            }
        } else {
            self.last_audio_ts_us.store(media_ts, Ordering::Relaxed);
        }
        
        // Update A/V skew
        self.update_av_skew();
        
        // Check alerts
        self.check_and_update_alerts();
    }
    
    fn update_lead_slope(&self) {
        let history = self.lead_history.lock().unwrap();
        if history.len() < 10 {
            return;
        }
        
        // Linear regression: slope = Σ(x-x̄)(y-ȳ) / Σ(x-x̄)²
        // Where x = arrival_ts, y = lead
        
        let n = history.len() as f64;
        let sum_x: f64 = history.iter().map(|s| s.arrival_ts_us as f64).sum();
        let sum_y: f64 = history.iter().map(|s| s.lead_us as f64).sum();
        let mean_x = sum_x / n;
        let mean_y = sum_y / n;
        
        let mut num = 0.0;
        let mut den = 0.0;
        for s in history.iter() {
            let dx = s.arrival_ts_us as f64 - mean_x;
            let dy = s.lead_us as f64 - mean_y;
            num += dx * dy;
            den += dx * dx;
        }
        
        if den > 0.0 {
            // slope is in μs per μs, convert to μs per second
            let slope_per_us = num / den;
            let slope_per_s = (slope_per_us * 1_000_000.0) as i64;
            self.lead_slope_us_per_s.store(slope_per_s, Ordering::Relaxed);
        }
    }
    
    fn record_cadence(&self, cadence_us: u64) {
        // Update histogram
        if let Ok(mut hist) = self.cadence_histogram.lock() {
            let _ = hist.record(cadence_us.min(1_000_000));
        }
        
        // Update running stats for variance
        self.cadence_count.fetch_add(1, Ordering::Relaxed);
        self.cadence_sum_us.fetch_add(cadence_us, Ordering::Relaxed);
        self.cadence_sum_sq.fetch_add(cadence_us * cadence_us, Ordering::Relaxed);
    }
    
    fn update_av_skew(&self) {
        let video_ts = self.last_video_ts_us.load(Ordering::Relaxed);
        let audio_ts = self.last_audio_ts_us.load(Ordering::Relaxed);
        
        if video_ts > 0 && audio_ts > 0 {
            let skew = video_ts as i64 - audio_ts as i64;
            self.av_skew_us.store(skew, Ordering::Relaxed);
        }
    }
    
    fn check_freeze(&self, content_hash: u64, arrival_ts: u64) {
        let last_hash = self.last_content_hash.swap(content_hash, Ordering::Relaxed);
        
        if content_hash == last_hash && last_hash != 0 {
            // Content unchanged - possibly frozen
            let frozen_count = self.frozen_frame_count.fetch_add(1, Ordering::Relaxed);
            
            if frozen_count == 0 {
                // Start of potential freeze
                self.freeze_start_us.store(arrival_ts, Ordering::Relaxed);
            }
        } else {
            // Content changed - end freeze if any
            let freeze_start = self.freeze_start_us.swap(0, Ordering::Relaxed);
            if freeze_start > 0 {
                let freeze_duration = arrival_ts.saturating_sub(freeze_start);
                self.total_freeze_duration_us.fetch_add(freeze_duration, Ordering::Relaxed);
            }
            self.frozen_frame_count.store(0, Ordering::Relaxed);
        }
    }
    
    fn check_and_update_alerts(&self) {
        let mut alerts = DriftAlerts::empty();
        
        // Check lead slope
        let slope = self.lead_slope_us_per_s.load(Ordering::Relaxed);
        if slope.abs() > self.thresholds.lead_slope_alert_us_per_s {
            alerts |= DriftAlerts::DRIFT_SLOPE;
        }
        
        // Check lead jump
        let current = self.lead_current_us.load(Ordering::Relaxed);
        let previous = self.lead_previous_us.load(Ordering::Relaxed);
        if (current - previous).abs() > self.thresholds.lead_jump_alert_us {
            alerts |= DriftAlerts::LEAD_JUMP;
        }
        
        // Check A/V skew
        let skew = self.av_skew_us.load(Ordering::Relaxed);
        if skew.abs() > self.thresholds.av_skew_alert_us {
            alerts |= DriftAlerts::AV_SKEW;
        }
        
        // Check freeze
        let freeze_start = self.freeze_start_us.load(Ordering::Relaxed);
        if freeze_start > 0 {
            let last_update = self.last_update_us.load(Ordering::Relaxed);
            let freeze_duration = last_update.saturating_sub(freeze_start);
            if freeze_duration > self.thresholds.freeze_alert_us {
                alerts |= DriftAlerts::FREEZE;
            }
        }
        
        // Check cadence variance
        if self.cadence_coefficient_of_variation() > self.thresholds.cadence_cv_alert {
            alerts |= DriftAlerts::CADENCE_UNSTABLE;
        }
        
        // Check health score
        if self.health_score() < self.thresholds.health_score_alert {
            alerts |= DriftAlerts::HEALTH_LOW;
        }
        
        self.active_alerts.store(alerts.bits(), Ordering::Relaxed);
    }
    
    /// Calculate cadence coefficient of variation (std_dev / mean)
    pub fn cadence_coefficient_of_variation(&self) -> f64 {
        let n = self.cadence_count.load(Ordering::Relaxed) as f64;
        if n < 2.0 {
            return 0.0;
        }
        
        let sum = self.cadence_sum_us.load(Ordering::Relaxed) as f64;
        let sum_sq = self.cadence_sum_sq.load(Ordering::Relaxed) as f64;
        
        let mean = sum / n;
        if mean == 0.0 {
            return 0.0;
        }
        
        let variance = (sum_sq / n) - (mean * mean);
        let std_dev = variance.max(0.0).sqrt();
        
        std_dev / mean
    }
    
    /// Compute health score (0.0 = bad, 1.0 = perfect)
    pub fn health_score(&self) -> f64 {
        let mut score = 1.0;
        
        // Penalize drift slope (max 30% penalty)
        let slope = self.lead_slope_us_per_s.load(Ordering::Relaxed).abs() as f64;
        let slope_penalty = (slope / 10_000.0).min(0.3);
        score -= slope_penalty;
        
        // Penalize A/V skew (max 20% penalty)
        let skew = self.av_skew_us.load(Ordering::Relaxed).abs() as f64;
        let skew_penalty = (skew / 200_000.0).min(0.2);
        score -= skew_penalty;
        
        // Penalize cadence variance (max 20% penalty)
        let cv = self.cadence_coefficient_of_variation();
        let cadence_penalty = (cv / 0.5).min(0.2);
        score -= cadence_penalty;
        
        // Penalize freeze time (max 30% penalty)
        let total_samples = self.total_samples.load(Ordering::Relaxed);
        let frozen_frames = self.frozen_frame_count.load(Ordering::Relaxed);
        let freeze_ratio = if total_samples > 0 {
            frozen_frames as f64 / total_samples as f64
        } else {
            0.0
        };
        let freeze_penalty = (freeze_ratio * 0.3).min(0.3);
        score -= freeze_penalty;
        
        score.max(0.0)
    }
    
    /// Get current alerts
    pub fn alerts(&self) -> DriftAlerts {
        DriftAlerts::from_bits_truncate(self.active_alerts.load(Ordering::Relaxed))
    }
    
    /// Export to Prometheus format
    pub fn to_prometheus(&self) -> String {
        let mut out = String::new();
        let id = &self.stream_id;
        
        // Lead metrics
        let lead = self.lead_current_us.load(Ordering::Relaxed);
        let slope = self.lead_slope_us_per_s.load(Ordering::Relaxed);
        out.push_str(&format!("stream_lead_us{{stream_id=\"{}\"}} {}\n", id, lead));
        out.push_str(&format!("stream_drift_slope_us_per_s{{stream_id=\"{}\"}} {}\n", id, slope));
        
        // A/V skew
        let skew = self.av_skew_us.load(Ordering::Relaxed);
        out.push_str(&format!("stream_av_skew_us{{stream_id=\"{}\"}} {}\n", id, skew));
        
        // Cadence percentiles
        if let Ok(hist) = self.cadence_histogram.lock() {
            out.push_str(&format!(
                "stream_cadence_p50_us{{stream_id=\"{}\"}} {}\n",
                id, hist.value_at_quantile(0.5)
            ));
            out.push_str(&format!(
                "stream_cadence_p95_us{{stream_id=\"{}\"}} {}\n",
                id, hist.value_at_quantile(0.95)
            ));
            out.push_str(&format!(
                "stream_cadence_p99_us{{stream_id=\"{}\"}} {}\n",
                id, hist.value_at_quantile(0.99)
            ));
        }
        
        // Cadence coefficient of variation
        let cv = self.cadence_coefficient_of_variation();
        out.push_str(&format!("stream_cadence_cv{{stream_id=\"{}\"}} {:.4}\n", id, cv));
        
        // Freeze metrics
        let freeze_duration = self.total_freeze_duration_us.load(Ordering::Relaxed);
        out.push_str(&format!("stream_freeze_duration_us{{stream_id=\"{}\"}} {}\n", id, freeze_duration));
        
        // Health score
        let health = self.health_score();
        out.push_str(&format!("stream_health_score{{stream_id=\"{}\"}} {:.4}\n", id, health));
        
        // Alerts (as gauge, 1 = active)
        let alerts = self.alerts();
        out.push_str(&format!(
            "stream_alert_drift_slope{{stream_id=\"{}\"}} {}\n",
            id, if alerts.contains(DriftAlerts::DRIFT_SLOPE) { 1 } else { 0 }
        ));
        out.push_str(&format!(
            "stream_alert_av_skew{{stream_id=\"{}\"}} {}\n",
            id, if alerts.contains(DriftAlerts::AV_SKEW) { 1 } else { 0 }
        ));
        out.push_str(&format!(
            "stream_alert_freeze{{stream_id=\"{}\"}} {}\n",
            id, if alerts.contains(DriftAlerts::FREEZE) { 1 } else { 0 }
        ));
        
        // Total samples
        let total = self.total_samples.load(Ordering::Relaxed);
        out.push_str(&format!("stream_total_samples{{stream_id=\"{}\"}} {}\n", id, total));
        
        out
    }
    
    /// Reset metrics (for window rotation)
    pub fn reset(&self) {
        *self.t0_arrival_us.lock().unwrap() = None;
        *self.s0_media_us.lock().unwrap() = None;
        self.lead_history.lock().unwrap().clear();
        self.lead_current_us.store(0, Ordering::Relaxed);
        self.lead_previous_us.store(0, Ordering::Relaxed);
        self.lead_slope_us_per_s.store(0, Ordering::Relaxed);
        self.last_media_ts_us.store(0, Ordering::Relaxed);
        if let Ok(mut hist) = self.cadence_histogram.lock() {
            hist.reset();
        }
        self.cadence_count.store(0, Ordering::Relaxed);
        self.cadence_sum_us.store(0, Ordering::Relaxed);
        self.cadence_sum_sq.store(0, Ordering::Relaxed);
        self.last_video_ts_us.store(0, Ordering::Relaxed);
        self.last_audio_ts_us.store(0, Ordering::Relaxed);
        self.av_skew_us.store(0, Ordering::Relaxed);
        self.last_content_hash.store(0, Ordering::Relaxed);
        self.freeze_start_us.store(0, Ordering::Relaxed);
        self.total_freeze_duration_us.store(0, Ordering::Relaxed);
        self.frozen_frame_count.store(0, Ordering::Relaxed);
        self.active_alerts.store(0, Ordering::Relaxed);
        self.total_samples.store(0, Ordering::Relaxed);
    }
}
```

#### 1.2 Add to executor module exports

**File**: `runtime-core/src/executor/mod.rs`

```rust
// Add to existing exports
pub mod drift_metrics;
pub use drift_metrics::{DriftMetrics, DriftAlerts, DriftThresholds};
```

#### 1.3 Add bitflags dependency

**File**: `runtime-core/Cargo.toml`

```toml
[dependencies]
# Add if not present
bitflags = "2.4"
```

---

### Phase 2: Executor Integration

#### 2.1 Add DriftMetrics to SessionRouter

**File**: `runtime-core/src/transport/session_router.rs`

```rust
use crate::executor::drift_metrics::DriftMetrics;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SessionRouter {
    // ... existing fields ...
    
    /// Per-stream drift metrics
    drift_metrics: Arc<RwLock<HashMap<String, Arc<DriftMetrics>>>>,
}

impl SessionRouter {
    // In route_to_node or equivalent method:
    
    async fn process_input(&self, node_id: &str, input: RuntimeData) -> Result<Vec<RuntimeData>> {
        // Extract timing for drift analysis
        let (media_ts, arrival_ts) = input.timing();
        let stream_id = input.stream_id().unwrap_or("default").to_string();
        let is_video = input.is_video();
        
        // Get or create drift metrics for this stream
        let drift_metrics = self.get_or_create_drift_metrics(&stream_id)?;
        
        // Record arrival (content_hash = None for now, can add later)
        drift_metrics.record_arrival(media_ts, arrival_ts, is_video, None);
        
        // Execute node (existing logic)
        let result = self.execute_node(node_id, input).await?;
        
        // Check for alerts and emit if needed
        let alerts = drift_metrics.alerts();
        if !alerts.is_empty() {
            self.emit_drift_alerts(&stream_id, alerts).await;
        }
        
        Ok(result)
    }
    
    fn get_or_create_drift_metrics(&self, stream_id: &str) -> Result<Arc<DriftMetrics>> {
        let metrics = self.drift_metrics.read().unwrap();
        if let Some(m) = metrics.get(stream_id) {
            return Ok(m.clone());
        }
        drop(metrics);
        
        let mut metrics = self.drift_metrics.write().unwrap();
        // Double-check after acquiring write lock
        if let Some(m) = metrics.get(stream_id) {
            return Ok(m.clone());
        }
        
        let m = Arc::new(DriftMetrics::new(stream_id)?);
        metrics.insert(stream_id.to_string(), m.clone());
        Ok(m)
    }
    
    async fn emit_drift_alerts(&self, stream_id: &str, alerts: DriftAlerts) {
        // Emit as control message or log
        tracing::warn!(
            stream_id = stream_id,
            alerts = ?alerts,
            "Drift alerts triggered"
        );
        
        // TODO: Could emit ControlMessage::Alert to pipeline
    }
    
    /// Get drift metrics for all streams (for Prometheus export)
    pub fn drift_metrics(&self) -> Vec<Arc<DriftMetrics>> {
        self.drift_metrics.read().unwrap().values().cloned().collect()
    }
}
```

#### 2.2 Add to StreamingNode trait (optional)

**File**: `runtime-core/src/nodes/streaming_node.rs`

For nodes that want access to drift metrics:

```rust
/// Optional: Nodes can implement this to receive drift metrics
pub trait DriftAwareNode {
    /// Called with current drift metrics before processing
    fn on_drift_update(&mut self, metrics: &DriftMetrics) {}
}
```

---

### Phase 3: Content Hash for Freeze Detection

#### 3.1 Simple perceptual hash utility

**File**: `runtime-core/src/data/content_hash.rs`

```rust
//! Simple perceptual hash for freeze detection
//!
//! Uses difference hash (dHash) on downscaled grayscale image.

/// Compute dHash of video frame data
/// 
/// # Arguments
/// * `pixel_data` - Raw pixel data (assumes RGB24 or similar)
/// * `width` - Frame width
/// * `height` - Frame height
/// 
/// # Returns
/// 64-bit hash value
pub fn dhash_video(pixel_data: &[u8], width: u32, height: u32) -> u64 {
    // For simplicity, sample a 9x8 grid from the frame
    // and compute horizontal gradient hash
    
    if pixel_data.is_empty() || width < 9 || height < 8 {
        return 0;
    }
    
    let mut hash: u64 = 0;
    let step_x = width as usize / 9;
    let step_y = height as usize / 8;
    let row_stride = (width as usize) * 3; // Assuming RGB24
    
    for y in 0..8 {
        for x in 0..8 {
            let idx1 = (y * step_y * row_stride) + (x * step_x * 3);
            let idx2 = (y * step_y * row_stride) + ((x + 1) * step_x * 3);
            
            if idx1 + 2 < pixel_data.len() && idx2 + 2 < pixel_data.len() {
                // Grayscale: 0.299*R + 0.587*G + 0.114*B
                let gray1 = (pixel_data[idx1] as u32 * 299 
                           + pixel_data[idx1 + 1] as u32 * 587 
                           + pixel_data[idx1 + 2] as u32 * 114) / 1000;
                let gray2 = (pixel_data[idx2] as u32 * 299 
                           + pixel_data[idx2 + 1] as u32 * 587 
                           + pixel_data[idx2 + 2] as u32 * 114) / 1000;
                
                if gray1 > gray2 {
                    hash |= 1 << (y * 8 + x);
                }
            }
        }
    }
    
    hash
}
```

---

### Phase 4: Prometheus/Metrics Export

#### 4.1 Extend metrics endpoint

**File**: `runtime-core/src/transport/runner.rs` (or wherever /metrics is served)

```rust
// In the metrics endpoint handler:

pub fn get_all_metrics(&self) -> String {
    let mut output = String::new();
    
    // Existing latency metrics
    for metrics in &self.latency_metrics {
        output.push_str(&metrics.to_prometheus());
    }
    
    // Add drift metrics
    for drift in self.session_router.drift_metrics() {
        output.push_str(&drift.to_prometheus());
    }
    
    output
}
```

---

## Testing Strategy

### Unit Tests

**File**: `runtime-core/src/executor/drift_metrics.rs` (in `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_metrics_creation() {
        let metrics = DriftMetrics::new("test_stream").unwrap();
        assert_eq!(metrics.stream_id, "test_stream");
        assert_eq!(metrics.health_score(), 1.0);
    }

    #[test]
    fn test_lead_calculation() {
        let metrics = DriftMetrics::new("test").unwrap();
        
        // First sample establishes baseline
        metrics.record_arrival(Some(0), Some(0), false, None);
        
        // Second sample: media at 1000μs, arrived at 1100μs
        // Lead = 1100 - 1000 = 100μs (arrival ahead)
        metrics.record_arrival(Some(1000), Some(1100), false, None);
        
        let lead = metrics.lead_current_us.load(Ordering::Relaxed);
        assert_eq!(lead, 100);
    }

    #[test]
    fn test_av_skew() {
        let metrics = DriftMetrics::new("test").unwrap();
        
        // Initialize
        metrics.record_arrival(Some(0), Some(0), true, None);
        
        // Video at 1000μs
        metrics.record_arrival(Some(1000), Some(1000), true, None);
        
        // Audio at 900μs (video ahead by 100μs)
        metrics.record_arrival(Some(900), Some(1000), false, None);
        
        let skew = metrics.av_skew_us.load(Ordering::Relaxed);
        assert_eq!(skew, 100);
    }

    #[test]
    fn test_cadence_variance() {
        let metrics = DriftMetrics::new("test").unwrap();
        
        metrics.record_arrival(Some(0), Some(0), false, None);
        
        // Uniform cadence: 33333μs (30fps)
        for i in 1..100 {
            metrics.record_arrival(Some(i * 33333), Some(i * 33333), false, None);
        }
        
        let cv = metrics.cadence_coefficient_of_variation();
        assert!(cv < 0.01, "CV should be near zero for uniform cadence");
    }

    #[test]
    fn test_health_score_degradation() {
        let metrics = DriftMetrics::new("test").unwrap();
        
        // Start with perfect health
        assert_eq!(metrics.health_score(), 1.0);
        
        // Simulate drift (increasing lead slope)
        metrics.lead_slope_us_per_s.store(10000, Ordering::Relaxed);
        
        let health = metrics.health_score();
        assert!(health < 1.0);
        assert!(health > 0.5);
    }

    #[test]
    fn test_alerts() {
        let metrics = DriftMetrics::with_thresholds(
            "test",
            DriftThresholds {
                lead_slope_alert_us_per_s: 1000,
                ..Default::default()
            }
        ).unwrap();
        
        // Trigger slope alert
        metrics.lead_slope_us_per_s.store(2000, Ordering::Relaxed);
        metrics.check_and_update_alerts();
        
        let alerts = metrics.alerts();
        assert!(alerts.contains(DriftAlerts::DRIFT_SLOPE));
    }

    #[test]
    fn test_prometheus_export() {
        let metrics = DriftMetrics::new("video_stream").unwrap();
        metrics.record_arrival(Some(0), Some(0), true, None);
        metrics.record_arrival(Some(33333), Some(33400), true, None);
        
        let prometheus = metrics.to_prometheus();
        
        assert!(prometheus.contains("stream_id=\"video_stream\""));
        assert!(prometheus.contains("stream_lead_us"));
        assert!(prometheus.contains("stream_health_score"));
    }
}
```

### Integration Tests

**File**: `runtime-core/tests/integration/test_drift_metrics.rs`

```rust
//! Integration tests for drift metrics in pipeline execution

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::executor::drift_metrics::DriftMetrics;

#[tokio::test]
async fn test_drift_metrics_in_pipeline() {
    // TODO: Test with actual pipeline execution
}

#[tokio::test]
async fn test_drift_alerts_emission() {
    // TODO: Test alert emission mechanism
}
```

---

## Task Checklist

### Phase 0: Prerequisites
- [ ] 0.1 Add `timestamp_us: Option<u64>` to RuntimeData::Audio
- [ ] 0.2 Add `arrival_ts_us: Option<u64>` to RuntimeData::Audio
- [ ] 0.3 Add `arrival_ts_us: Option<u64>` to RuntimeData::Video
- [ ] 0.4 Add `timing()` and `stream_id()` helper methods to RuntimeData
- [ ] 0.5 Create `AudioTimingTracker` utility
- [ ] 0.6 Update IPC serialization for new fields (if needed)
- [ ] 0.7 Update any tests that construct Audio/Video variants

### Phase 1: Core Implementation
- [ ] 1.1 Add `bitflags` to Cargo.toml dependencies
- [ ] 1.2 Create `drift_metrics.rs` with DriftMetrics struct
- [ ] 1.3 Implement lead/drift calculation
- [ ] 1.4 Implement cadence tracking with histogram
- [ ] 1.5 Implement A/V skew tracking
- [ ] 1.6 Implement freeze detection
- [ ] 1.7 Implement health score calculation
- [ ] 1.8 Implement alert threshold checking
- [ ] 1.9 Implement Prometheus export
- [ ] 1.10 Export from executor/mod.rs

### Phase 2: Integration
- [ ] 2.1 Add DriftMetrics storage to SessionRouter
- [ ] 2.2 Call record_arrival() in input processing path
- [ ] 2.3 Add alert emission mechanism
- [ ] 2.4 Add drift_metrics() accessor for export

### Phase 3: Content Hash (Optional)
- [ ] 3.1 Create content_hash.rs with dhash_video()
- [ ] 3.2 Integrate hash into record_arrival() calls for video

### Phase 4: Export & Observability
- [ ] 4.1 Extend /metrics endpoint with drift metrics
- [ ] 4.2 Add Grafana dashboard JSON (optional)

### Phase 5: Testing
- [ ] 5.1 Unit tests for DriftMetrics
- [ ] 5.2 Unit tests for AudioTimingTracker
- [ ] 5.3 Integration test with pipeline
- [ ] 5.4 Performance benchmark (<1μs per sample)

### Phase 6: Documentation
- [ ] 6.1 Update CLAUDE.md with drift metrics info
- [ ] 6.2 Add docstrings to public APIs
- [ ] 6.3 Example usage in docs/

---

## Open Questions

1. **Should drift metrics be per-node or per-stream?**
   - Current plan: Per-stream (identified by stream_id)
   - Alternative: Per-node for more granular tracking

2. **Alert delivery mechanism?**
   - Options: ControlMessage, tracing::warn, callback, SSE
   - Current plan: tracing + optional ControlMessage

3. **Window rotation for metrics?**
   - Current: Single cumulative window with reset()
   - Could add: Rolling windows like latency_metrics

4. **Content hash for freeze detection?**
   - Simple dHash is fast but may miss subtle changes
   - Could add: aHash, pHash, or SSIM for better accuracy

---

## References

- `runtime-core/src/executor/latency_metrics.rs` - Existing HDR histogram pattern
- `runtime-core/src/executor/metrics.rs` - Existing pipeline metrics
- `runtime-core/src/lib.rs:130` - RuntimeData enum definition
- `runtime-core/src/transport/session_router.rs` - Integration point
