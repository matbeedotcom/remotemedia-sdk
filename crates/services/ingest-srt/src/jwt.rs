//! JWT token validation and generation
//!
//! This module handles JWT tokens used for authenticating SRT connections.
//! Tokens are embedded in the SRT streamid and validated when connections are accepted.

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT token claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenClaims {
    /// Session ID this token is valid for
    pub session_id: String,

    /// Expiration timestamp (Unix epoch)
    pub exp: i64,

    /// Token scopes (e.g., ["ingest:srt"])
    #[serde(default)]
    pub scopes: Vec<String>,

    /// Maximum bitrate in bits per second
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bitrate: Option<u64>,

    /// Maximum frames per second
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fps: Option<u32>,

    /// Maximum session duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<u64>,
}

impl TokenClaims {
    /// Create new token claims
    pub fn new(session_id: String, ttl_seconds: i64) -> Self {
        let exp = Utc::now()
            .checked_add_signed(Duration::seconds(ttl_seconds))
            .expect("valid timestamp")
            .timestamp();

        Self {
            session_id,
            exp,
            scopes: vec!["ingest:srt".to_string()],
            max_bitrate: None,
            max_fps: None,
            max_duration: None,
        }
    }

    /// Set the maximum bitrate
    pub fn with_max_bitrate(mut self, max_bitrate: u64) -> Self {
        self.max_bitrate = Some(max_bitrate);
        self
    }

    /// Set the maximum FPS
    pub fn with_max_fps(mut self, max_fps: u32) -> Self {
        self.max_fps = Some(max_fps);
        self
    }

    /// Set the maximum duration
    pub fn with_max_duration(mut self, max_duration: u64) -> Self {
        self.max_duration = Some(max_duration);
        self
    }

    /// Check if the token has expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }
}

/// JWT validator for SRT ingest tokens
pub struct JwtValidator {
    secret: String,
}

impl JwtValidator {
    /// Create a new JWT validator with the given secret
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    /// Validate and decode a JWT token
    ///
    /// # Arguments
    /// * `token` - The JWT token string to validate
    ///
    /// # Errors
    /// * `JwtError::Expired` - Token has expired
    /// * `JwtError::InvalidSignature` - Token signature is invalid
    /// * `JwtError::InvalidFormat` - Token format is invalid
    pub fn validate(&self, token: &str) -> Result<TokenClaims, JwtError> {
        let key = DecodingKey::from_secret(self.secret.as_bytes());
        let mut validation = Validation::default();
        // No leeway for expiration checks - tokens expire at exactly the exp time
        validation.leeway = 0;

        let token_data = decode::<TokenClaims>(token, &key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::Expired,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => JwtError::InvalidSignature,
                _ => JwtError::InvalidFormat(e.to_string()),
            })?;

        Ok(token_data.claims)
    }

    /// Validate a token and check that it matches the expected session ID
    ///
    /// # Arguments
    /// * `token` - The JWT token string to validate
    /// * `expected_session_id` - The session ID from the streamid
    ///
    /// # Errors
    /// * `JwtError::SessionMismatch` - Token session_id doesn't match expected
    pub fn validate_for_session(
        &self,
        token: &str,
        expected_session_id: &str,
    ) -> Result<TokenClaims, JwtError> {
        let claims = self.validate(token)?;

        if claims.session_id != expected_session_id {
            return Err(JwtError::SessionMismatch {
                expected: expected_session_id.to_string(),
                actual: claims.session_id,
            });
        }

        Ok(claims)
    }

    /// Generate a new JWT token for a session
    ///
    /// # Arguments
    /// * `session_id` - The session ID to generate a token for
    /// * `ttl_seconds` - Token time-to-live in seconds
    pub fn generate(&self, session_id: &str, ttl_seconds: i64) -> Result<String, JwtError> {
        let claims = TokenClaims::new(session_id.to_string(), ttl_seconds);
        self.generate_with_claims(claims)
    }

    /// Generate a new JWT token with custom claims
    pub fn generate_with_claims(&self, claims: TokenClaims) -> Result<String, JwtError> {
        let key = EncodingKey::from_secret(self.secret.as_bytes());

        encode(&Header::default(), &claims, &key)
            .map_err(|e| JwtError::Generation(e.to_string()))
    }
}

/// JWT-related errors
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum JwtError {
    #[error("Token has expired")]
    Expired,

    #[error("Invalid token signature")]
    InvalidSignature,

    #[error("Invalid token format: {0}")]
    InvalidFormat(String),

    #[error("Session ID mismatch: expected {expected}, got {actual}")]
    SessionMismatch { expected: String, actual: String },

    #[error("Token generation failed: {0}")]
    Generation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key-for-unit-tests";

    #[test]
    fn test_generate_and_validate_token() {
        let validator = JwtValidator::new(TEST_SECRET.to_string());

        let token = validator.generate("sess_123", 3600).unwrap();
        let claims = validator.validate(&token).unwrap();

        assert_eq!(claims.session_id, "sess_123");
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_validate_expired_token() {
        let validator = JwtValidator::new(TEST_SECRET.to_string());

        // Generate a token that's already expired
        let claims = TokenClaims {
            session_id: "sess_123".to_string(),
            exp: Utc::now().timestamp() - 1, // 1 second ago
            scopes: vec!["ingest:srt".to_string()],
            max_bitrate: None,
            max_fps: None,
            max_duration: None,
        };

        let token = validator.generate_with_claims(claims).unwrap();
        let result = validator.validate(&token);

        assert_eq!(result.unwrap_err(), JwtError::Expired);
    }

    #[test]
    fn test_validate_wrong_secret() {
        let generator = JwtValidator::new(TEST_SECRET.to_string());
        let validator = JwtValidator::new("wrong-secret".to_string());

        let token = generator.generate("sess_123", 3600).unwrap();
        let result = validator.validate(&token);

        assert_eq!(result.unwrap_err(), JwtError::InvalidSignature);
    }

    #[test]
    fn test_validate_for_session() {
        let validator = JwtValidator::new(TEST_SECRET.to_string());

        let token = validator.generate("sess_123", 3600).unwrap();

        // Correct session ID
        let result = validator.validate_for_session(&token, "sess_123");
        assert!(result.is_ok());

        // Wrong session ID
        let result = validator.validate_for_session(&token, "sess_456");
        match result.unwrap_err() {
            JwtError::SessionMismatch { expected, actual } => {
                assert_eq!(expected, "sess_456");
                assert_eq!(actual, "sess_123");
            }
            _ => panic!("Expected SessionMismatch error"),
        }
    }

    #[test]
    fn test_token_claims_with_limits() {
        let claims = TokenClaims::new("sess_123".to_string(), 3600)
            .with_max_bitrate(5_000_000)
            .with_max_fps(30)
            .with_max_duration(900);

        assert_eq!(claims.max_bitrate, Some(5_000_000));
        assert_eq!(claims.max_fps, Some(30));
        assert_eq!(claims.max_duration, Some(900));

        let validator = JwtValidator::new(TEST_SECRET.to_string());
        let token = validator.generate_with_claims(claims.clone()).unwrap();
        let decoded = validator.validate(&token).unwrap();

        assert_eq!(decoded.max_bitrate, claims.max_bitrate);
        assert_eq!(decoded.max_fps, claims.max_fps);
        assert_eq!(decoded.max_duration, claims.max_duration);
    }

    #[test]
    fn test_invalid_token_format() {
        let validator = JwtValidator::new(TEST_SECRET.to_string());
        let result = validator.validate("not-a-valid-jwt");

        match result {
            Err(JwtError::InvalidFormat(_)) => {}
            _ => panic!("Expected InvalidFormat error"),
        }
    }
}
