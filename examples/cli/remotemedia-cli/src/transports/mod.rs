//! Transport wrappers for CLI

pub mod grpc;
pub mod http;
pub mod webrtc;

use anyhow::Result;
use url::Url;

/// Transport type based on URL scheme
#[derive(Debug, Clone, Copy)]
pub enum TransportType {
    Grpc,
    Http,
    WebRTC,
}

/// Detect transport type from URL
pub fn detect_transport(url: &str) -> Result<TransportType> {
    let parsed = Url::parse(url)?;
    match parsed.scheme() {
        "grpc" | "grpcs" => Ok(TransportType::Grpc),
        "http" | "https" => Ok(TransportType::Http),
        "ws" | "wss" => Ok(TransportType::WebRTC),
        scheme => anyhow::bail!("Unknown URL scheme: {}", scheme),
    }
}

/// Get the host and port from a URL
pub fn parse_endpoint(url: &str) -> Result<(String, u16)> {
    let parsed = Url::parse(url)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Missing host in URL"))?
        .to_string();
    let port = parsed.port().unwrap_or_else(|| {
        match parsed.scheme() {
            "grpc" | "http" | "ws" => 80,
            "grpcs" | "https" | "wss" => 443,
            _ => 8080,
        }
    });
    Ok((host, port))
}
