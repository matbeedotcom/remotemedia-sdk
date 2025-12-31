//! License validation and activation
//!
//! This module handles license key parsing, validation, and persistence.
//! License keys use Ed25519 signatures for offline validation.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// License validation errors
#[derive(Error, Debug)]
pub enum LicenseError {
    #[error("Invalid license key format. Expected RMDA-XXXX-XXXX-XXXX")]
    InvalidFormat,
    
    #[error("Invalid license signature")]
    InvalidSignature,
    
    #[error("License has expired")]
    Expired,
    
    #[error("Failed to read license file: {0}")]
    ReadError(String),
    
    #[error("Failed to save license file: {0}")]
    SaveError(String),
}

/// License plan tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LicensePlan {
    /// Starter plan (3 concurrent streams)
    Starter,
    /// Pro plan (10 concurrent streams)
    Pro,
    /// Enterprise plan (unlimited streams)
    Enterprise,
}

impl LicensePlan {
    /// Get the maximum number of concurrent streams for this plan
    pub fn max_streams(&self) -> u32 {
        match self {
            Self::Starter => 3,
            Self::Pro => 10,
            Self::Enterprise => u32::MAX,
        }
    }

    /// Get the display name for this plan
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Starter => "Starter",
            Self::Pro => "Pro",
            Self::Enterprise => "Enterprise",
        }
    }
}

/// License structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// License key in RMDA-XXXX-XXXX-XXXX format
    pub key: String,
    /// License plan tier
    pub plan: LicensePlan,
    /// When the license was issued
    pub issued_at: DateTime<Utc>,
    /// When the license expires (None for lifetime)
    pub expires_at: Option<DateTime<Utc>>,
    /// Maximum concurrent streams
    pub max_streams: u32,
    /// Ed25519 signature (base64 encoded)
    pub signature: String,
}

impl License {
    /// Check if the license has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false // Never expires
        }
    }

    /// Validate the license signature
    /// 
    /// For now, this is a placeholder that always returns Ok.
    /// In production, this would verify the Ed25519 signature.
    pub fn validate(&self) -> Result<(), LicenseError> {
        // TODO: Implement actual signature verification
        // For now, accept any properly formatted key
        if !self.key.starts_with("RMDA-") {
            return Err(LicenseError::InvalidFormat);
        }

        if self.is_expired() {
            return Err(LicenseError::Expired);
        }

        Ok(())
    }

    /// Parse a license key string into a License
    /// 
    /// For now, this creates a placeholder license for testing.
    /// In production, this would decode the actual license data from the key.
    pub fn from_key(key: &str) -> Result<Self, LicenseError> {
        // Validate format: RMDA-XXXX-XXXX-XXXX
        let parts: Vec<&str> = key.split('-').collect();
        if parts.len() != 4 || parts[0] != "RMDA" {
            return Err(LicenseError::InvalidFormat);
        }

        // For now, create a placeholder license
        // In production, we would decode the key data
        Ok(Self {
            key: key.to_string(),
            plan: LicensePlan::Pro, // Default to Pro for testing
            issued_at: Utc::now(),
            expires_at: None, // Never expires for testing
            max_streams: 10,
            signature: String::new(), // Placeholder
        })
    }
}

/// Load license from disk
pub fn load_license() -> Result<License, LicenseError> {
    let path = get_license_path()
        .ok_or_else(|| LicenseError::ReadError("No config directory".to_string()))?;

    if !path.exists() {
        return Err(LicenseError::ReadError("License file not found".to_string()));
    }

    let contents = std::fs::read_to_string(&path)
        .map_err(|e| LicenseError::ReadError(e.to_string()))?;

    let license: License = serde_json::from_str(&contents)
        .map_err(|e| LicenseError::ReadError(e.to_string()))?;

    license.validate()?;
    Ok(license)
}

/// Save license to disk
pub fn save_license(license: &License) -> Result<(), LicenseError> {
    let path = get_license_path()
        .ok_or_else(|| LicenseError::SaveError("No config directory".to_string()))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LicenseError::SaveError(e.to_string()))?;
    }

    let contents = serde_json::to_string_pretty(license)
        .map_err(|e| LicenseError::SaveError(e.to_string()))?;

    std::fs::write(&path, contents)
        .map_err(|e| LicenseError::SaveError(e.to_string()))?;

    Ok(())
}

/// Activate a license key
pub fn activate_license(key: &str) -> Result<License, LicenseError> {
    let license = License::from_key(key)?;
    license.validate()?;
    save_license(&license)?;
    Ok(license)
}

/// Handle the activate subcommand
pub fn activate_license_command(key: &str) -> Result<()> {
    println!("Activating license key: {}...", key);
    
    match activate_license(key) {
        Ok(license) => {
            println!();
            println!("✓ License activated successfully!");
            println!();
            println!("  Plan:        {}", license.plan.display_name());
            println!("  Max streams: {}", license.max_streams);
            if let Some(expires) = license.expires_at {
                println!("  Expires:     {}", expires.format("%Y-%m-%d"));
            } else {
                println!("  Expires:     Never");
            }
            println!();
            println!("You can now run remotemedia-demo without time limits.");
            Ok(())
        }
        Err(e) => {
            eprintln!();
            eprintln!("✗ License activation failed: {}", e);
            eprintln!();
            eprintln!("Please check your license key and try again.");
            eprintln!("For help, visit: https://remotemedia.dev/support");
            Err(anyhow::anyhow!("License activation failed"))
        }
    }
}

/// Get the platform-specific path for license file
fn get_license_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("remotemedia").join("license.key"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_license_plan_max_streams() {
        assert_eq!(LicensePlan::Starter.max_streams(), 3);
        assert_eq!(LicensePlan::Pro.max_streams(), 10);
        assert_eq!(LicensePlan::Enterprise.max_streams(), u32::MAX);
    }

    // T085: Valid key parsing
    #[test]
    fn test_license_from_key_valid() {
        let license = License::from_key("RMDA-TEST-1234-5678").unwrap();
        assert_eq!(license.key, "RMDA-TEST-1234-5678");
        assert!(!license.is_expired());
    }

    // T085 continuation: Key with different parts
    #[test]
    fn test_license_from_key_various_valid() {
        let cases = [
            "RMDA-AAAA-BBBB-CCCC",
            "RMDA-1234-5678-9012",
            "RMDA-xxxx-yyyy-zzzz",
        ];

        for key in cases {
            let license = License::from_key(key);
            assert!(license.is_ok(), "Key '{}' should be valid", key);
        }
    }

    // T086: Invalid key rejection
    #[test]
    fn test_license_from_key_invalid_format() {
        assert!(matches!(
            License::from_key("INVALID-KEY"),
            Err(LicenseError::InvalidFormat)
        ));
        
        assert!(matches!(
            License::from_key("XXXX-TEST-1234-5678"),
            Err(LicenseError::InvalidFormat)
        ));
    }

    // T086 continuation: Various invalid formats
    #[test]
    fn test_license_from_key_various_invalid() {
        let invalid_cases = [
            "",                           // Empty
            "RMDA",                        // Too short
            "RMDA-XXXX",                   // Missing parts
            "RMDA-XXXX-YYYY",             // Missing part
            "rmda-xxxx-yyyy-zzzz",        // Lowercase prefix (wrong format)
            "RMDA-XXXX-YYYY-ZZZZ-EXTRA",  // Too many parts
            "ABC-XXXX-YYYY-ZZZZ",          // Wrong prefix
        ];

        for key in invalid_cases {
            let result = License::from_key(key);
            assert!(result.is_err(), "Key '{}' should be invalid", key);
        }
    }

    // T087: Expired license detection
    #[test]
    fn test_license_is_expired() {
        // Not expired (future date)
        let future_license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Pro,
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(30)),
            max_streams: 10,
            signature: "test".to_string(),
        };
        assert!(!future_license.is_expired());

        // Expired (past date)
        let expired_license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Pro,
            issued_at: Utc::now() - chrono::Duration::days(60),
            expires_at: Some(Utc::now() - chrono::Duration::days(30)),
            max_streams: 10,
            signature: "test".to_string(),
        };
        assert!(expired_license.is_expired());

        // Never expires (None)
        let lifetime_license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Enterprise,
            issued_at: Utc::now(),
            expires_at: None,
            max_streams: u32::MAX,
            signature: "test".to_string(),
        };
        assert!(!lifetime_license.is_expired());
    }

    // T087 continuation: Expired license validation fails
    #[test]
    fn test_expired_license_validation_fails() {
        let expired_license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Pro,
            issued_at: Utc::now() - chrono::Duration::days(60),
            expires_at: Some(Utc::now() - chrono::Duration::days(30)),
            max_streams: 10,
            signature: "test".to_string(),
        };

        let result = expired_license.validate();
        assert!(matches!(result, Err(LicenseError::Expired)));
    }

    // T088: Signature verification (placeholder - validates key format only)
    #[test]
    fn test_license_validate_with_valid_signature() {
        let license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Pro,
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(30)),
            max_streams: 10,
            signature: "valid_signature_placeholder".to_string(),
        };

        // Should pass validation (placeholder implementation)
        assert!(license.validate().is_ok());
    }

    // T088 continuation: Invalid format fails validation
    #[test]
    fn test_license_validate_with_invalid_format() {
        let license = License {
            key: "INVALID-KEY".to_string(), // Wrong format
            plan: LicensePlan::Pro,
            issued_at: Utc::now(),
            expires_at: None,
            max_streams: 10,
            signature: "test".to_string(),
        };

        let result = license.validate();
        assert!(matches!(result, Err(LicenseError::InvalidFormat)));
    }

    #[test]
    fn test_license_serialization() {
        let license = License {
            key: "RMDA-TEST-1234-5678".to_string(),
            plan: LicensePlan::Pro,
            issued_at: Utc::now(),
            expires_at: None,
            max_streams: 10,
            signature: "test".to_string(),
        };

        let json = serde_json::to_string(&license).unwrap();
        let parsed: License = serde_json::from_str(&json).unwrap();

        assert_eq!(license.key, parsed.key);
        assert_eq!(license.plan, parsed.plan);
        assert_eq!(license.max_streams, parsed.max_streams);
    }

    #[test]
    fn test_license_plan_display_name() {
        assert_eq!(LicensePlan::Starter.display_name(), "Starter");
        assert_eq!(LicensePlan::Pro.display_name(), "Pro");
        assert_eq!(LicensePlan::Enterprise.display_name(), "Enterprise");
    }
}
