//! Client configuration types
//!
//! Defines the main configuration structures for the SocksRat client.

use serde::{Deserialize, Serialize};
use super::{PoolConfig, TransportConfig};

/// Default heartbeat timeout in seconds
fn default_heartbeat_timeout() -> u64 {
    40
}

/// Root configuration structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Client configuration
    pub client: ClientConfig,
}

/// Client configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
    /// Remote rathole server address (e.g., "server.example.com:2333")
    pub remote_addr: String,

    /// Service name for the SOCKS5 tunnel
    pub service_name: String,

    /// Authentication token
    pub token: String,

    /// Transport configuration
    #[serde(default)]
    pub transport: TransportConfig,

    /// Heartbeat timeout in seconds
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout: u64,

    /// SOCKS5 server configuration
    #[serde(default)]
    pub socks: SocksConfig,

    /// Connection pool configuration
    #[serde(default)]
    pub pool: PoolConfig,
}

/// Default DNS resolve setting
fn default_dns_resolve() -> bool {
    true
}

/// Default request timeout in seconds
fn default_request_timeout() -> u64 {
    10
}

/// SOCKS5 server configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SocksConfig {
    /// Enable/disable authentication
    #[serde(default)]
    pub auth_required: bool,

    /// Username for SOCKS5 auth
    #[serde(default)]
    pub username: Option<String>,

    /// Password for SOCKS5 auth
    #[serde(default)]
    pub password: Option<String>,

    /// Allow UDP associate command
    #[serde(default)]
    pub allow_udp: bool,

    /// DNS resolution mode (true = resolve on client side)
    #[serde(default = "default_dns_resolve")]
    pub dns_resolve: bool,

    /// Request timeout in seconds
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
}

impl Default for SocksConfig {
    fn default() -> Self {
        Self {
            auth_required: false,
            username: None,
            password: None,
            allow_udp: false,
            dns_resolve: default_dns_resolve(),
            request_timeout: default_request_timeout(),
        }
    }
}

impl SocksConfig {
    /// Check if authentication credentials are configured
    pub fn has_credentials(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.auth_required && !self.has_credentials() {
            return Err("Authentication required but no credentials configured".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks_config_default() {
        let config = SocksConfig::default();
        assert!(!config.auth_required);
        assert!(config.dns_resolve);
        assert_eq!(config.request_timeout, 10);
        assert!(!config.allow_udp);
    }

    #[test]
    fn test_socks_config_has_credentials() {
        let config = SocksConfig {
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };
        assert!(config.has_credentials());

        let config = SocksConfig {
            username: Some("user".to_string()),
            password: None,
            ..Default::default()
        };
        assert!(!config.has_credentials());
    }

    #[test]
    fn test_socks_config_validate() {
        let config = SocksConfig {
            auth_required: true,
            username: None,
            password: None,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = SocksConfig {
            auth_required: true,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };
        assert!(config.validate().is_ok());

        let config = SocksConfig {
            auth_required: false,
            username: None,
            password: None,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_heartbeat_timeout() {
        assert_eq!(default_heartbeat_timeout(), 40);
    }
}
