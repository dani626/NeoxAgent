use serde::Deserialize;
use std::path::PathBuf;

/// Root configuration loaded from config.toml
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub agent: AgentConfig,
    pub podman: PodmanConfig,
    pub tls: TlsConfig,
    pub defaults: DefaultsConfig,
    pub backups: BackupsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub host: String,
    pub port: u16,
    pub api_key: String,
    pub data_dir: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PodmanConfig {
    pub socket: String,
    pub volumes_dir: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DefaultsConfig {
    pub restart_policy: String,
    pub dns: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackupsConfig {
    pub max_per_server: u32,
    pub max_size_gb: u32,
    pub retention_days: u32,
    pub compression_level: u32,
    pub stop_server_before_backup: bool,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load from default path (config.toml in current directory)
    pub fn load_default() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load("config.toml")
    }
}
