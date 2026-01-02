//! Demo mode banners and warnings
//!
//! This module provides the user-facing UI elements for demo mode,
//! including startup banners, time warnings, and limit messages.

use std::time::Duration;

/// Display the demo mode startup banner
pub fn show_startup_banner(sessions_remaining: u32, session_duration: Duration) {
    let minutes = session_duration.as_secs() / 60;
    let seconds = session_duration.as_secs() % 60;
    let sessions_used = 3u32.saturating_sub(sessions_remaining);

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║  RemoteMedia Demo • Stream Health Monitor                        ║");
    eprintln!("║  ─────────────────────────────────────────────────────────────── ║");
    eprintln!("║  Session: {:02}:{:02} remaining (demo mode)                          ║", minutes, seconds);
    eprintln!("║  Sessions today: {}/3                                              ║", sessions_used + 1);
    eprintln!("║                                                                  ║");
    eprintln!("║  Get a license at https://remotemedia.dev/license                ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");
    eprintln!();
}

/// Display the licensed mode startup banner
pub fn show_licensed_banner() {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║  RemoteMedia • Stream Health Monitor (Licensed)                  ║");
    eprintln!("║  ─────────────────────────────────────────────────────────────── ║");
    eprintln!("║  Running in licensed mode - no time limits                       ║");
    eprintln!("║  Press Ctrl+C to stop                                            ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");
    eprintln!();
}

/// Show the 1-minute warning before session timeout
pub fn show_warning(remaining: Duration) {
    eprintln!();
    eprintln!("⏰ Demo session ending in {} seconds", remaining.as_secs());
    eprintln!();
}

/// Show the daily limit reached message
#[allow(dead_code)] // Called when daily limit is reached
pub fn show_daily_limit_reached(resets_in: Duration) {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║  Daily demo limit reached (3/3 sessions)                         ║");
    eprintln!("║                                                                   ║");
    eprintln!("║  Your demo resets in {}.                                   ║", format_duration(resets_in));
    eprintln!("║                                                                   ║");
    eprintln!("║  Ready for unlimited monitoring?                                  ║");
    eprintln!("║  → https://remotemedia.dev/license                               ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");
    eprintln!();
}

/// Format a duration as human-readable string
#[allow(dead_code)] // Used by show_daily_limit_reached
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    
    if total_secs < 60 {
        format!("{} second{}", total_secs, if total_secs == 1 { "" } else { "s" })
    } else if total_secs < 3600 {
        let minutes = total_secs / 60;
        format!("{} minute{}", minutes, if minutes == 1 { "" } else { "s" })
    } else {
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        if minutes > 0 {
            format!("{} hour{} {} minute{}", 
                hours, if hours == 1 { "" } else { "s" },
                minutes, if minutes == 1 { "" } else { "s" })
        } else {
            format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1 second");
        assert_eq!(format_duration(Duration::from_secs(30)), "30 seconds");
        assert_eq!(format_duration(Duration::from_secs(59)), "59 seconds");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1 minute");
        assert_eq!(format_duration(Duration::from_secs(120)), "2 minutes");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59 minutes");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1 hour");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2 hours");
        assert_eq!(format_duration(Duration::from_secs(3660)), "1 hour 1 minute");
        assert_eq!(format_duration(Duration::from_secs(7320)), "2 hours 2 minutes");
    }
}
