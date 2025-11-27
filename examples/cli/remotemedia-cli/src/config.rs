//! Configuration management for RemoteMedia CLI

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Default transport type
    #[serde(default = "default_transport")]
    pub default_transport: String,

    /// Output format
    #[serde(default = "default_output_format")]
    pub output_format: String,

    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Default server URL
    pub default_server: Option<String>,

    /// Audio settings
    #[serde(default)]
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AudioConfig {
    /// Sample rate
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    /// Channels
    #[serde(default = "default_channels")]
    pub channels: u16,

    /// Buffer size in milliseconds
    #[serde(default = "default_buffer_size_ms")]
    pub buffer_size_ms: u32,
}

fn default_transport() -> String {
    "grpc".to_string()
}

fn default_output_format() -> String {
    "text".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_sample_rate() -> u32 {
    48000
}

fn default_channels() -> u16 {
    1
}

fn default_buffer_size_ms() -> u32 {
    20
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_transport: default_transport(),
            output_format: default_output_format(),
            log_level: default_log_level(),
            default_server: None,
            audio: AudioConfig::default(),
        }
    }
}

/// Server entry in servers.toml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerEntry {
    pub name: String,
    pub url: String,
    pub auth_token: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

/// Get the config directory path
fn get_config_dir() -> Result<PathBuf> {
    let dir = directories::ProjectDirs::from("com", "remotemedia", "cli")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            directories::BaseDirs::new()
                .map(|d| d.home_dir().join(".remotemedia"))
                .unwrap_or_else(|| PathBuf::from(".").join(".remotemedia"))
        });

    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Load configuration from file or defaults
pub fn load_config(custom_path: Option<&str>) -> Result<Config> {
    let config_path = match custom_path {
        Some(path) => PathBuf::from(path),
        None => get_config_dir()?.join("config.toml"),
    };

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    } else {
        Ok(Config::default())
    }
}

/// Load servers from servers.toml
pub fn load_servers() -> Result<Vec<ServerEntry>> {
    let servers_path = get_config_dir()?.join("servers.toml");

    if servers_path.exists() {
        let content = std::fs::read_to_string(&servers_path)?;
        #[derive(Deserialize)]
        struct ServersFile {
            #[serde(default)]
            servers: Vec<ServerEntry>,
        }
        let file: ServersFile = toml::from_str(&content)?;
        Ok(file.servers)
    } else {
        Ok(Vec::new())
    }
}

/// Save servers to servers.toml
pub fn save_servers(servers: &[ServerEntry]) -> Result<()> {
    let servers_path = get_config_dir()?.join("servers.toml");

    #[derive(Serialize)]
    struct ServersFile<'a> {
        servers: &'a [ServerEntry],
    }

    let content = toml::to_string_pretty(&ServersFile { servers })?;
    std::fs::write(&servers_path, content)?;

    Ok(())
}

/// Get the default server if configured
pub fn get_default_server(config: &Config) -> Option<ServerEntry> {
    if let Some(ref url) = config.default_server {
        return Some(ServerEntry {
            name: "default".to_string(),
            url: url.clone(),
            auth_token: None,
            is_default: true,
        });
    }

    load_servers().ok().and_then(|servers| {
        servers.into_iter().find(|s| s.is_default)
    })
}
