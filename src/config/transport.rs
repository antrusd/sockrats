//! Transport configuration types
//!
//! Defines configuration for different transport protocols (TCP, Noise).

use serde::{Deserialize, Serialize};

/// Transport type enumeration
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportType {
    /// Plain TCP transport
    #[default]
    #[serde(rename = "tcp")]
    Tcp,
    /// Noise protocol encrypted transport
    #[serde(rename = "noise")]
    Noise,
}

/// Main transport configuration
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TransportConfig {
    /// Transport type
    #[serde(rename = "type", default)]
    pub transport_type: TransportType,

    /// TCP configuration
    #[serde(default)]
    pub tcp: TcpConfig,

    /// Noise protocol configuration (optional)
    #[serde(default)]
    pub noise: Option<NoiseConfig>,
}

/// Default keepalive seconds
fn default_keepalive_secs() -> u64 {
    20
}

/// Default keepalive interval
fn default_keepalive_interval() -> u64 {
    8
}

/// TCP transport configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TcpConfig {
    /// Enable TCP_NODELAY
    #[serde(default)]
    pub nodelay: bool,

    /// TCP keepalive timeout in seconds
    #[serde(default = "default_keepalive_secs")]
    pub keepalive_secs: u64,

    /// TCP keepalive interval in seconds
    #[serde(default = "default_keepalive_interval")]
    pub keepalive_interval: u64,
}

impl Default for TcpConfig {
    fn default() -> Self {
        TcpConfig {
            nodelay: true,
            keepalive_secs: default_keepalive_secs(),
            keepalive_interval: default_keepalive_interval(),
        }
    }
}

/// Noise protocol configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NoiseConfig {
    /// Noise protocol pattern
    #[serde(default = "default_noise_pattern")]
    pub pattern: String,

    /// Local private key (base64 encoded)
    pub local_private_key: Option<String>,

    /// Remote public key (base64 encoded)
    pub remote_public_key: String,
}

fn default_noise_pattern() -> String {
    "Noise_NK_25519_ChaChaPoly_BLAKE2s".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_default() {
        assert_eq!(TransportType::default(), TransportType::Tcp);
    }

    #[test]
    fn test_tcp_config_default() {
        let config = TcpConfig::default();
        assert!(config.nodelay);
        assert_eq!(config.keepalive_secs, 20);
        assert_eq!(config.keepalive_interval, 8);
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.transport_type, TransportType::Tcp);
        assert!(config.noise.is_none());
    }

    #[test]
    fn test_noise_pattern_default() {
        assert_eq!(default_noise_pattern(), "Noise_NK_25519_ChaChaPoly_BLAKE2s");
    }
}
