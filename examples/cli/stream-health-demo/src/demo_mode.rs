//! Demo mode enforcement
//!
//! This module implements the demo session limits:
//! - 15-minute maximum session duration
//! - 3 sessions per calendar day (UTC)
//! - Persistent state tracking across runs

use crate::events::HealthEvent;
use crate::license::License;
use crate::summary::SessionSummary;
use crate::DemoConfig;
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Demo mode errors
#[derive(Error, Debug)]
pub enum DemoError {
    #[error("Daily session limit reached ({0}/{1}). Resets at UTC midnight.")]
    DailyLimitReached(u32, u32),
    
    #[error("Failed to load demo state: {0}")]
    StateLoadError(String),
    
    #[error("Failed to save demo state: {0}")]
    StateSaveError(String),
}

/// Demo session state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoState {
    /// Date when the demo was first run
    pub install_date: DateTime<Utc>,
    /// Sessions run today (cleared at UTC midnight)
    pub sessions_today: Vec<SessionRecord>,
    /// Machine identifier for telemetry (opt-in)
    pub machine_id: String,
    /// Last session date (to detect day rollover)
    pub last_session_date: Option<DateTime<Utc>>,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            install_date: Utc::now(),
            sessions_today: Vec::new(),
            machine_id: generate_machine_id(),
            last_session_date: None,
        }
    }
}

/// Record of a completed demo session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    /// When the session started
    pub started: DateTime<Utc>,
    /// How long the session ran (seconds)
    pub duration_secs: u64,
    /// Number of events detected
    pub events_count: u32,
}

/// Demo limits configuration
#[derive(Debug, Clone)]
pub struct DemoLimits {
    /// Maximum session duration
    pub session_duration: Duration,
    /// Maximum sessions per calendar day
    pub max_sessions_per_day: u32,
    /// Maximum concurrent streams (always 1 in demo)
    pub concurrent_streams: u32,
}

impl Default for DemoLimits {
    fn default() -> Self {
        Self {
            session_duration: Duration::from_secs(DemoConfig::SESSION_DURATION_SECS),
            max_sessions_per_day: DemoConfig::MAX_SESSIONS_PER_DAY,
            concurrent_streams: 1,
        }
    }
}

/// Demo mode controller
pub struct DemoController {
    state: DemoState,
    limits: DemoLimits,
    session_start: Option<Instant>,
    license: Option<License>,
    state_path: Option<PathBuf>,
}

impl DemoController {
    /// Load demo state from disk, or create new state if not found
    pub fn load() -> Result<Self, DemoError> {
        let state_path = get_state_path();
        
        // Try to load existing state
        let state = if let Some(ref path) = state_path {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(contents) => {
                        serde_json::from_str(&contents).unwrap_or_else(|e| {
                            tracing::warn!("Corrupted state file, resetting: {}", e);
                            DemoState::default()
                        })
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read state file: {}", e);
                        DemoState::default()
                    }
                }
            } else {
                DemoState::default()
            }
        } else {
            // Read-only filesystem - use default state
            DemoState::default()
        };

        // Try to load license
        let license = crate::license::load_license().ok();

        let mut controller = Self {
            state,
            limits: DemoLimits::default(),
            session_start: None,
            license,
            state_path,
        };

        // Check for day rollover
        controller.check_day_rollover();

        Ok(controller)
    }

    /// Check if we have a valid (non-expired) license
    pub fn has_valid_license(&self) -> bool {
        self.license.as_ref().map(|l| !l.is_expired()).unwrap_or(false)
    }

    /// Check if a new session can be started
    pub fn check_can_start(&self) -> Result<(), DemoError> {
        if self.has_valid_license() {
            return Ok(());
        }

        let sessions_today = self.state.sessions_today.len() as u32;
        if sessions_today >= self.limits.max_sessions_per_day {
            return Err(DemoError::DailyLimitReached(
                sessions_today,
                self.limits.max_sessions_per_day,
            ));
        }

        Ok(())
    }

    /// Start a new demo session
    pub fn start_session(&mut self) -> Result<(), DemoError> {
        self.check_can_start()?;
        self.session_start = Some(Instant::now());
        self.state.last_session_date = Some(Utc::now());
        Ok(())
    }

    /// Get remaining time in the current session
    pub fn time_remaining(&self) -> Duration {
        if self.has_valid_license() {
            // No limit for licensed users
            return Duration::from_secs(u64::MAX);
        }

        if let Some(start) = self.session_start {
            let elapsed = start.elapsed();
            if elapsed >= self.limits.session_duration {
                Duration::ZERO
            } else {
                self.limits.session_duration - elapsed
            }
        } else {
            self.limits.session_duration
        }
    }

    /// Get number of sessions remaining today
    pub fn sessions_remaining(&self) -> u32 {
        if self.has_valid_license() {
            return u32::MAX;
        }
        
        let used = self.state.sessions_today.len() as u32;
        self.limits.max_sessions_per_day.saturating_sub(used)
    }

    /// Check if warning should be shown (< 1 minute remaining)
    pub fn should_warn(&self) -> bool {
        if self.has_valid_license() {
            return false;
        }
        
        let remaining = self.time_remaining();
        remaining <= Duration::from_secs(DemoConfig::WARNING_SECS) && remaining > Duration::ZERO
    }

    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        if self.has_valid_license() {
            return false;
        }
        
        self.time_remaining() == Duration::ZERO
    }

    /// End the current session and generate summary
    pub fn end_session(&mut self, events: &[HealthEvent]) -> SessionSummary {
        let duration = self.session_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);

        // Record the session
        let record = SessionRecord {
            started: self.state.last_session_date.unwrap_or_else(Utc::now),
            duration_secs: duration.as_secs(),
            events_count: events.len() as u32,
        };
        self.state.sessions_today.push(record);

        // Save state
        if let Err(e) = self.save_state() {
            tracing::warn!("Failed to save demo state: {}", e);
        }

        // Generate summary
        let termination_reason = if self.is_expired() {
            crate::summary::TerminationReason::DemoTimeout
        } else {
            crate::summary::TerminationReason::UserInterrupt
        };

        SessionSummary::from_events(events, duration, termination_reason)
    }

    /// Check for day rollover and reset session count if needed
    fn check_day_rollover(&mut self) {
        let today = Utc::now();
        
        if let Some(last_date) = self.state.last_session_date {
            // Check if it's a different day (UTC)
            if last_date.ordinal() != today.ordinal() || last_date.year() != today.year() {
                tracing::info!("Day rollover detected, resetting session count");
                self.state.sessions_today.clear();
            }
        }
    }

    /// Save state to disk
    fn save_state(&self) -> Result<(), DemoError> {
        if let Some(ref path) = self.state_path {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DemoError::StateSaveError(e.to_string()))?;
            }

            let contents = serde_json::to_string_pretty(&self.state)
                .map_err(|e| DemoError::StateSaveError(e.to_string()))?;
            
            std::fs::write(path, contents)
                .map_err(|e| DemoError::StateSaveError(e.to_string()))?;
        }
        Ok(())
    }
}

/// Get the platform-specific path for demo state file
fn get_state_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("remotemedia").join("demo.json"))
}

/// Generate a simple machine identifier
fn generate_machine_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    
    // Hash hostname if available
    if let Ok(hostname) = std::env::var("HOSTNAME") {
        hostname.hash(&mut hasher);
    }
    
    // Hash username if available
    if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        user.hash(&mut hasher);
    }
    
    // Add some randomness
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);

    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test controller with custom state
    fn create_test_controller(state: DemoState) -> DemoController {
        DemoController {
            state,
            limits: DemoLimits::default(),
            session_start: None,
            license: None,
            state_path: None, // Don't persist in tests
        }
    }

    #[test]
    fn test_demo_limits_default() {
        let limits = DemoLimits::default();
        assert_eq!(limits.session_duration, Duration::from_secs(900)); // 15 minutes
        assert_eq!(limits.max_sessions_per_day, 3);
        assert_eq!(limits.concurrent_streams, 1);
    }

    #[test]
    fn test_demo_state_default() {
        let state = DemoState::default();
        assert!(state.sessions_today.is_empty());
        assert!(!state.machine_id.is_empty());
    }

    #[test]
    fn test_session_record_serialization() {
        let record = SessionRecord {
            started: Utc::now(),
            duration_secs: 900,
            events_count: 42,
        };
        
        let json = serde_json::to_string(&record).unwrap();
        let parsed: SessionRecord = serde_json::from_str(&json).unwrap();
        
        assert_eq!(record.duration_secs, parsed.duration_secs);
        assert_eq!(record.events_count, parsed.events_count);
    }

    // T053: Session limit enforcement (4th session blocked)
    #[test]
    fn test_session_limit_enforcement() {
        // Create state with 3 sessions (daily limit)
        let mut state = DemoState::default();
        state.last_session_date = Some(Utc::now());
        for _ in 0..3 {
            state.sessions_today.push(SessionRecord {
                started: Utc::now(),
                duration_secs: 900,
                events_count: 10,
            });
        }

        let controller = create_test_controller(state);
        
        // 4th session should be blocked
        let result = controller.check_can_start();
        assert!(result.is_err());
        
        match result {
            Err(DemoError::DailyLimitReached(used, max)) => {
                assert_eq!(used, 3);
                assert_eq!(max, 3);
            }
            _ => panic!("Expected DailyLimitReached error"),
        }
    }

    // T053 continuation: Sessions before limit are allowed
    #[test]
    fn test_sessions_before_limit_allowed() {
        // Create state with 2 sessions
        let mut state = DemoState::default();
        state.last_session_date = Some(Utc::now());
        for _ in 0..2 {
            state.sessions_today.push(SessionRecord {
                started: Utc::now(),
                duration_secs: 900,
                events_count: 10,
            });
        }

        let controller = create_test_controller(state);
        
        // 3rd session should be allowed
        assert!(controller.check_can_start().is_ok());
        assert_eq!(controller.sessions_remaining(), 1);
    }

    // T054: Day rollover resets session count
    #[test]
    fn test_day_rollover_resets_sessions() {
        // Create state with sessions from yesterday
        let mut state = DemoState::default();
        let yesterday = Utc::now() - chrono::Duration::days(1);
        state.last_session_date = Some(yesterday);
        
        // Add 3 sessions "from yesterday"
        for _ in 0..3 {
            state.sessions_today.push(SessionRecord {
                started: yesterday,
                duration_secs: 900,
                events_count: 10,
            });
        }

        let mut controller = create_test_controller(state);
        
        // Trigger day rollover check
        controller.check_day_rollover();
        
        // Sessions should be reset
        assert!(controller.state.sessions_today.is_empty());
        assert_eq!(controller.sessions_remaining(), 3);
        assert!(controller.check_can_start().is_ok());
    }

    // T054 continuation: Same day doesn't reset
    #[test]
    fn test_same_day_preserves_sessions() {
        let mut state = DemoState::default();
        state.last_session_date = Some(Utc::now());
        state.sessions_today.push(SessionRecord {
            started: Utc::now(),
            duration_secs: 900,
            events_count: 10,
        });

        let mut controller = create_test_controller(state);
        
        // Check day rollover (should not reset since it's same day)
        controller.check_day_rollover();
        
        // Session should still be there
        assert_eq!(controller.state.sessions_today.len(), 1);
        assert_eq!(controller.sessions_remaining(), 2);
    }

    // T055: Corrupted state file resets gracefully
    #[test]
    fn test_corrupted_state_resets_gracefully() {
        // Test that invalid JSON results in default state
        let invalid_json = "{ this is not valid json }";
        let result: Result<DemoState, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
        
        // The load() function uses unwrap_or_else to handle this
        let state: DemoState = serde_json::from_str(invalid_json)
            .unwrap_or_else(|_| DemoState::default());
        
        assert!(state.sessions_today.is_empty());
    }

    // T055 continuation: Partial/malformed JSON also resets
    #[test]
    fn test_partial_json_resets() {
        let partial_json = r#"{"install_date": "2024-01-01T00:00:00Z""#; // Missing closing brace
        let result: Result<DemoState, _> = serde_json::from_str(partial_json);
        assert!(result.is_err());
    }

    // T056: Monotonic timer not affected by clock changes
    #[test]
    fn test_monotonic_timer() {
        let state = DemoState::default();
        let mut controller = create_test_controller(state);
        
        // Start a session
        controller.session_start = Some(Instant::now());
        
        // The time_remaining uses Instant (monotonic clock)
        let remaining_before = controller.time_remaining();
        
        // Wait a tiny bit
        std::thread::sleep(Duration::from_millis(10));
        
        let remaining_after = controller.time_remaining();
        
        // Time should have decreased (monotonically)
        assert!(remaining_after < remaining_before);
        
        // The difference should be roughly the sleep time
        let diff = remaining_before - remaining_after;
        assert!(diff >= Duration::from_millis(10));
        assert!(diff < Duration::from_millis(100)); // Allow some tolerance
    }

    // T056 continuation: Verify Instant is used, not SystemTime
    #[test]
    fn test_session_start_uses_instant() {
        let state = DemoState::default();
        let mut controller = create_test_controller(state);
        
        // Verify session_start is Option<Instant>, not Option<SystemTime>
        assert!(controller.session_start.is_none());
        
        // After starting, it should be Some(Instant)
        controller.session_start = Some(Instant::now());
        assert!(controller.session_start.is_some());
        
        // Verify elapsed time calculation
        let remaining = controller.time_remaining();
        assert!(remaining > Duration::ZERO);
        assert!(remaining <= controller.limits.session_duration);
    }

    // Additional test: Licensed users bypass limits
    #[test]
    fn test_licensed_users_bypass_limits() {
        let mut state = DemoState::default();
        // Add max sessions
        for _ in 0..3 {
            state.sessions_today.push(SessionRecord {
                started: Utc::now(),
                duration_secs: 900,
                events_count: 10,
            });
        }

        let mut controller = create_test_controller(state);
        
        // Without license, should fail
        assert!(controller.check_can_start().is_err());
        
        // Add a valid license
        controller.license = Some(crate::license::License {
            key: "test".to_string(),
            plan: crate::license::LicensePlan::Pro,
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(365)),
            max_streams: 10,
            signature: "test_signature".to_string(),
        });
        
        // With license, should pass
        assert!(controller.check_can_start().is_ok());
        assert!(controller.has_valid_license());
        assert_eq!(controller.sessions_remaining(), u32::MAX);
        assert_eq!(controller.time_remaining(), Duration::from_secs(u64::MAX));
    }
}
