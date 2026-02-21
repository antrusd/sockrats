//! Transport configuration types
//!
//! Defines configuration for different transport protocols (TCP, TLS, Noise, WebSocket).

use serde::{Deserialize, Serialize};

/// Transport type enumeration
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportType {
    /// Plain TCP transport
    #[default]
    #[serde(rename = "tcp")]
    Tcp,
    /// TLS encrypted transport
    #[serde(rename = "tls")]
    Tls,
    /// Noise protocol encrypted transport
    #[serde(rename = "noise")]
    Noise,
    /// WebSocket transport
    #[serde(rename = "websocket")]
    Websocket,
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

    /// TLS configuration (optional)
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    /// Noise protocol configuration (optional)
    #[serde(default)]
    pub noise: Option<NoiseConfig>,

    /// WebSocket configuration (optional)
    #[serde(default)]
    pub websocket: Option<WebsocketConfig>,
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

/// TLS transport configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TlsConfig {
    /// Server hostname for verification
    pub hostname: Option<String>,

    /// Path to trusted root certificate
    pub trusted_root: Option<String>,

    /// Skip certificate verification (dangerous!)
    #[serde(default)]
    pub skip_verify: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        TlsConfig {
            hostname: None,
            trusted_root: None,
            skip_verify: false,
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

/// WebSocket transport configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebsocketConfig {
    /// WebSocket URL path
    #[serde(default = "default_ws_path")]
    pub path: String,

    /// Custom headers
    #[serde(default)]
    pub headers: Vec<(String, String)>,

    /// Enable TLS for WebSocket (wss://)
    #[serde(default)]
    pub tls: bool,

    /// TLS configuration if enabled
    pub tls_config: Option<TlsConfig>,
}

fn default_ws_path() -> String {
    "/".to_string()
}

impl Default for WebsocketConfig {
    fn default() -> Self {
        WebsocketConfig {
            path: default_ws_path(),
            headers: Vec::new(),
            tls: false,
            tls_config: None,
        }
    }
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
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert!(config.hostname.is_none());
        assert!(config.trusted_root.is_none());
        assert!(!config.skip_verify);
    }

    #[test]
    fn test_websocket_config_default() {
        let config = WebsocketConfig::default();
        assert_eq!(config.path, "/");
        assert!(config.headers.is_empty());
        assert!(!config.tls);
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.transport_type, TransportType::Tcp);
        assert!(config.tls.is_none());
        assert!(config.noise.is_none());
        assert!(config.websocket.is_none());
    }

    #[test]
    fn test_noise_pattern_default() {
        assert_eq!(
            default_noise_pattern(),
            "Noise_NK_25519_ChaChaPoly_BLAKE2s"
        );
    }
}
