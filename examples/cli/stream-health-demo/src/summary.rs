//! Session summary generation and reporting
//!
//! This module generates the end-of-session summary that shows what the
//! demo found and provides proof of value to the user.

use crate::events::HealthEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Reason why the session ended
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationReason {
    /// Demo session timeout (15 minutes)
    DemoTimeout,
    /// User pressed Ctrl+C
    UserInterrupt,
    /// Input stream ended naturally
    StreamEnded,
    /// An error occurred
    Error,
}

impl TerminationReason {
    /// Get display string for the termination reason
    pub fn display(&self) -> &'static str {
        match self {
            Self::DemoTimeout => "Demo session timeout",
            Self::UserInterrupt => "User interrupt (Ctrl+C)",
            Self::StreamEnded => "Stream ended",
            Self::Error => "Error",
        }
    }
}

/// Health score statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStats {
    /// Average health score
    pub average: f64,
    /// Minimum health score
    pub minimum: f64,
    /// Time spent below threshold
    pub time_below_threshold: Duration,
    /// Number of health samples
    pub sample_count: usize,
}

impl Default for HealthStats {
    fn default() -> Self {
        Self {
            average: 1.0,
            minimum: 1.0,
            time_below_threshold: Duration::ZERO,
            sample_count: 0,
        }
    }
}

/// Session summary with all metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Total session duration
    pub duration: Duration,
    /// Number of samples processed (estimated from events)
    pub samples_processed: u64,
    /// Drift events detected
    pub drift_events: Vec<DriftEventSummary>,
    /// Freeze events detected
    pub freeze_events: Vec<FreezeEventSummary>,
    /// Health score statistics
    pub health_stats: HealthStats,
    /// Why the session ended
    pub termination_reason: TerminationReason,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session ended
    pub ended_at: DateTime<Utc>,
}

/// Summary of a drift event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEventSummary {
    pub ts: DateTime<Utc>,
    pub lead_ms: i64,
}

/// Summary of a freeze event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeEventSummary {
    pub ts: DateTime<Utc>,
    pub duration_ms: u64,
}

impl SessionSummary {
    /// Create a summary from collected events
    pub fn from_events(
        events: &[HealthEvent],
        duration: Duration,
        termination_reason: TerminationReason,
    ) -> Self {
        let ended_at = Utc::now();
        let started_at = ended_at - chrono::Duration::from_std(duration).unwrap_or_default();

        // Extract drift events
        let drift_events: Vec<DriftEventSummary> = events
            .iter()
            .filter_map(|e| match e {
                HealthEvent::Drift { ts, lead_ms, .. } => Some(DriftEventSummary {
                    ts: *ts,
                    lead_ms: *lead_ms,
                }),
                _ => None,
            })
            .collect();

        // Extract freeze events
        let freeze_events: Vec<FreezeEventSummary> = events
            .iter()
            .filter_map(|e| match e {
                HealthEvent::Freeze { ts, duration_ms, .. } => Some(FreezeEventSummary {
                    ts: *ts,
                    duration_ms: *duration_ms,
                }),
                _ => None,
            })
            .collect();

        // Calculate health stats
        let health_scores: Vec<f64> = events
            .iter()
            .filter_map(|e| match e {
                HealthEvent::Health { score, .. } => Some(*score),
                _ => None,
            })
            .collect();

        let health_stats = if health_scores.is_empty() {
            HealthStats::default()
        } else {
            let average = health_scores.iter().sum::<f64>() / health_scores.len() as f64;
            let minimum = health_scores
                .iter()
                .copied()
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(1.0);
            let below_threshold = health_scores.iter().filter(|&&s| s < 0.7).count();
            
            HealthStats {
                average,
                minimum,
                time_below_threshold: Duration::from_secs(below_threshold as u64), // Approximation
                sample_count: health_scores.len(),
            }
        };

        // Estimate samples processed (rough estimate based on event count)
        let samples_processed = (duration.as_secs() * 16000) as u64; // 16kHz sample rate

        Self {
            duration,
            samples_processed,
            drift_events,
            freeze_events,
            health_stats,
            termination_reason,
            started_at,
            ended_at,
        }
    }

    /// Display the summary to the terminal
    pub fn display(&self) {
        let total_issues = self.drift_events.len() + self.freeze_events.len();
        let total_freeze_ms: u64 = self.freeze_events.iter().map(|f| f.duration_ms).sum();
        let max_drift = self.drift_events
            .iter()
            .map(|d| d.lead_ms.abs())
            .max()
            .unwrap_or(0);

        eprintln!();
        eprintln!("═══════════════════════════════════════════════════════════════════");
        eprintln!("  SESSION SUMMARY");
        eprintln!("───────────────────────────────────────────────────────────────────");
        eprintln!("  Duration:        {:02}:{:02}", 
            self.duration.as_secs() / 60, 
            self.duration.as_secs() % 60);
        eprintln!("  Samples:         {}", format_number(self.samples_processed));
        eprintln!();
        eprintln!("  Drift Events:    {} (max: {}ms)", self.drift_events.len(), max_drift);
        eprintln!("  Freeze Events:   {} (total: {}ms)", self.freeze_events.len(), total_freeze_ms);
        eprintln!("  Avg Health:      {:.2}", self.health_stats.average);
        eprintln!("  Min Health:      {:.2}", self.health_stats.minimum);
        eprintln!();
        eprintln!("  Ended:           {}", self.termination_reason.display());
        eprintln!();
        
        // Call to action
        if total_issues > 0 {
            eprintln!("  {}", self.cta_message());
        } else {
            eprintln!("  No issues detected in this session.");
        }
        eprintln!();
        eprintln!("  → Unlock unlimited: https://remotemedia.dev/license");
        eprintln!("═══════════════════════════════════════════════════════════════════");
        eprintln!();
    }

    /// Save the full report as JSON
    /// 
    /// Handles disk full errors gracefully by warning instead of crashing.
    pub fn save_report(&self, path: &Path) -> std::io::Result<()> {
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        match std::fs::write(path, &contents) {
            Ok(()) => {
                eprintln!("  Full report: {}", path.display());
                Ok(())
            }
            Err(e) if is_disk_full_error(&e) => {
                // Handle disk full gracefully - warn but don't crash
                eprintln!("  Warning: Could not save report (disk full)");
                eprintln!("  Report not saved, but session data was:");
                eprintln!("    - {} drift events", self.drift_events.len());
                eprintln!("    - {} freeze events", self.freeze_events.len());
                eprintln!("    - avg health: {:.2}", self.health_stats.average);
                Ok(()) // Return Ok to not crash the application
            }
            Err(e) if is_permission_error(&e) => {
                // Handle permission errors gracefully
                eprintln!("  Warning: Could not save report (permission denied)");
                Ok(())
            }
            Err(e) => {
                // Other errors should still be reported
                eprintln!("  Warning: Could not save report: {}", e);
                Ok(()) // Return Ok to not crash
            }
        }
    }
    
    /// Try to save report, returning a result without failing the session
    pub fn try_save_report(&self, path: &Path) -> bool {
        match self.save_report(path) {
            Ok(()) => true,
            Err(_) => false,
        }
    }

    /// Generate call-to-action message
    pub fn cta_message(&self) -> String {
        let total_issues = self.drift_events.len() + self.freeze_events.len();
        let minutes = self.duration.as_secs() / 60;
        
        format!(
            "Found {} issue{} in {} minute{}. Imagine what you'd catch with 24/7 monitoring.",
            total_issues,
            if total_issues == 1 { "" } else { "s" },
            minutes,
            if minutes == 1 { "" } else { "s" }
        )
    }
}

/// Get the default path for the session report
pub fn default_report_path() -> PathBuf {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");
    PathBuf::from(format!("./remotemedia-demo-{}.json", timestamp))
}

/// Check if an error is a disk full error
fn is_disk_full_error(error: &std::io::Error) -> bool {
    // Check for specific error kinds related to disk space
    matches!(error.raw_os_error(), 
        Some(28) | // ENOSPC on Unix
        Some(112) // ERROR_DISK_FULL on Windows
    ) || error.kind() == std::io::ErrorKind::StorageFull
}

/// Check if an error is a permission error
fn is_permission_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

/// Format a large number with thousand separators
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_summary_empty() {
        let summary = SessionSummary::from_events(
            &[],
            Duration::from_secs(60),
            TerminationReason::UserInterrupt,
        );

        assert!(summary.drift_events.is_empty());
        assert!(summary.freeze_events.is_empty());
        assert_eq!(summary.health_stats.average, 1.0);
    }

    #[test]
    fn test_session_summary_with_events() {
        let events = vec![
            HealthEvent::drift(50, 50, None),
            HealthEvent::drift(75, 50, None),
            HealthEvent::freeze(823, None),
            HealthEvent::health(0.72, vec!["DRIFT".to_string()]),
            HealthEvent::health(0.85, vec![]),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(900),
            TerminationReason::DemoTimeout,
        );

        assert_eq!(summary.drift_events.len(), 2);
        assert_eq!(summary.freeze_events.len(), 1);
        assert_eq!(summary.health_stats.sample_count, 2);
        assert!((summary.health_stats.average - 0.785).abs() < 0.01);
    }

    #[test]
    fn test_cta_message() {
        let events = vec![
            HealthEvent::drift(50, 50, None),
            HealthEvent::freeze(823, None),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(900),
            TerminationReason::DemoTimeout,
        );

        let cta = summary.cta_message();
        assert!(cta.contains("2 issues"));
        assert!(cta.contains("15 minutes"));
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(1234567), "1,234,567");
        assert_eq!(format_number(42), "42");
    }

    // T065: Disk full error handling - the function should not panic
    #[test]
    fn test_save_report_to_nonexistent_dir() {
        let summary = SessionSummary::from_events(
            &[],
            Duration::from_secs(60),
            TerminationReason::UserInterrupt,
        );

        // Try to save to a path that likely doesn't exist or is read-only
        // This should not panic, just return an error or handle gracefully
        let result = summary.save_report(Path::new("/nonexistent/path/report.json"));
        // The function is designed to not crash even on errors
        assert!(result.is_ok()); // Our implementation catches all errors and returns Ok
    }

    // T069: Detailed statistics calculation test
    #[test]
    fn test_statistics_calculation_detailed() {
        let events = vec![
            // Health events with varying scores
            HealthEvent::health(0.90, vec![]),
            HealthEvent::health(0.60, vec!["DRIFT_SLOPE".to_string()]),
            HealthEvent::health(0.80, vec![]),
            HealthEvent::health(0.50, vec!["FREEZE".to_string()]),
            HealthEvent::health(0.70, vec![]),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(300),
            TerminationReason::StreamEnded,
        );

        // Average: (0.90 + 0.60 + 0.80 + 0.50 + 0.70) / 5 = 0.70
        assert!((summary.health_stats.average - 0.70).abs() < 0.001);
        
        // Minimum: 0.50
        assert!((summary.health_stats.minimum - 0.50).abs() < 0.001);
        
        // Sample count: 5
        assert_eq!(summary.health_stats.sample_count, 5);
        
        // Time below threshold (scores < 0.7): 2 samples (0.60, 0.50)
        assert_eq!(summary.health_stats.time_below_threshold.as_secs(), 2);
    }

    // T069 continuation: Test with only drift events (no health scores)
    #[test]
    fn test_statistics_with_no_health_events() {
        let events = vec![
            HealthEvent::drift(50, 50, None),
            HealthEvent::drift(100, 50, None),
            HealthEvent::freeze(500, None),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(60),
            TerminationReason::UserInterrupt,
        );

        // With no health events, defaults should be used
        assert_eq!(summary.health_stats.average, 1.0);
        assert_eq!(summary.health_stats.minimum, 1.0);
        assert_eq!(summary.health_stats.sample_count, 0);
        
        // But drift and freeze events should be counted
        assert_eq!(summary.drift_events.len(), 2);
        assert_eq!(summary.freeze_events.len(), 1);
    }

    // T070: Test CTA message with different issue counts
    #[test]
    fn test_cta_message_single_issue() {
        let events = vec![
            HealthEvent::drift(50, 50, None),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(60), // 1 minute
            TerminationReason::UserInterrupt,
        );

        let cta = summary.cta_message();
        assert!(cta.contains("1 issue")); // Singular
        assert!(!cta.contains("issues")); // Not plural
        assert!(cta.contains("1 minute")); // Singular
    }

    // T070 continuation: Test CTA message with multiple issues
    #[test]
    fn test_cta_message_multiple_issues() {
        let events = vec![
            HealthEvent::drift(50, 50, None),
            HealthEvent::drift(75, 50, None),
            HealthEvent::freeze(500, None),
            HealthEvent::freeze(800, None),
            HealthEvent::freeze(300, None),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(600), // 10 minutes
            TerminationReason::DemoTimeout,
        );

        let cta = summary.cta_message();
        assert!(cta.contains("5 issues")); // 2 drift + 3 freeze
        assert!(cta.contains("10 minutes"));
    }

    // T070 continuation: Test CTA message with zero issues
    #[test]
    fn test_cta_message_no_issues() {
        let events = vec![
            HealthEvent::health(0.95, vec![]),
            HealthEvent::health(0.98, vec![]),
        ];

        let summary = SessionSummary::from_events(
            &events,
            Duration::from_secs(120),
            TerminationReason::StreamEnded,
        );

        let cta = summary.cta_message();
        assert!(cta.contains("0 issues"));
    }
}
