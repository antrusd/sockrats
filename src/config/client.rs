//! Client configuration types
//!
//! Defines the main configuration structures for the Sockrats client.

use super::{PoolConfig, TransportConfig};
use crate::services::ssh::SshConfig;
use serde::{Deserialize, Serialize};

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

/// Service type for multi-service support
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    /// SOCKS5 proxy service
    #[default]
    Socks5,
    /// SSH server service
    Ssh,
}

/// Service configuration for a single service
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceConfig {
    /// Service name (must match rathole server config)
    pub name: String,

    /// Service type
    #[serde(default)]
    pub service_type: ServiceType,

    /// Authentication token
    pub token: String,

    /// SOCKS5 configuration (used when service_type is Socks5)
    #[serde(default)]
    pub socks: Option<SocksConfig>,

    /// SSH configuration (used when service_type is Ssh)
    #[serde(default)]
    pub ssh: Option<SshConfig>,
}

/// Helper functions for service list operations
pub trait ServiceListExt {
    /// Get service by name
    fn get_service(&self, name: &str) -> Option<&ServiceConfig>;
    /// Get all SOCKS5 services
    fn socks_services(&self) -> Vec<&ServiceConfig>;
    /// Get all SSH services
    fn ssh_services(&self) -> Vec<&ServiceConfig>;
}

impl ServiceListExt for Vec<ServiceConfig> {
    fn get_service(&self, name: &str) -> Option<&ServiceConfig> {
        self.iter().find(|s| s.name == name)
    }

    fn socks_services(&self) -> Vec<&ServiceConfig> {
        self.iter()
            .filter(|s| s.service_type == ServiceType::Socks5)
            .collect()
    }

    fn ssh_services(&self) -> Vec<&ServiceConfig> {
        self.iter()
            .filter(|s| s.service_type == ServiceType::Ssh)
            .collect()
    }
}

/// Client configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
    /// Remote rathole server address (e.g., "server.example.com:2333")
    pub remote_addr: String,

    /// Service name for the SOCKS5 tunnel (legacy single-service mode)
    #[serde(default)]
    pub service_name: String,

    /// Authentication token (legacy single-service mode)
    #[serde(default)]
    pub token: String,

    /// Transport configuration
    #[serde(default)]
    pub transport: TransportConfig,

    /// Heartbeat timeout in seconds
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout: u64,

    /// SOCKS5 server configuration (legacy single-service mode)
    #[serde(default)]
    pub socks: SocksConfig,

    /// SSH server configuration (legacy single-service mode)
    #[serde(default)]
    pub ssh: SshConfig,

    /// Connection pool configuration
    #[serde(default)]
    pub pool: PoolConfig,

    /// Multi-service configuration (array of services)
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
}

impl ClientConfig {
    /// Check if using multi-service mode
    pub fn is_multi_service(&self) -> bool {
        !self.services.is_empty()
    }

    /// Get effective services (either from multi-service or legacy single-service)
    pub fn effective_services(&self) -> Vec<ServiceConfig> {
        if self.is_multi_service() {
            self.services.clone()
        } else {
            // Legacy single-service mode
            vec![ServiceConfig {
                name: self.service_name.clone(),
                service_type: ServiceType::Socks5,
                token: self.token.clone(),
                socks: Some(self.socks.clone()),
                ssh: None,
            }]
        }
    }
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

    #[test]
    fn test_service_type_default() {
        let service_type = ServiceType::default();
        assert_eq!(service_type, ServiceType::Socks5);
    }

    #[test]
    fn test_service_type_eq() {
        assert_eq!(ServiceType::Socks5, ServiceType::Socks5);
        assert_eq!(ServiceType::Ssh, ServiceType::Ssh);
        assert_ne!(ServiceType::Socks5, ServiceType::Ssh);
    }

    #[test]
    fn test_service_list_empty() {
        let services: Vec<ServiceConfig> = Vec::new();
        assert!(services.is_empty());
    }

    #[test]
    fn test_service_list_get_service() {
        let services = vec![
            ServiceConfig {
                name: "socks".to_string(),
                service_type: ServiceType::Socks5,
                token: "token1".to_string(),
                socks: Some(SocksConfig::default()),
                ssh: None,
            },
            ServiceConfig {
                name: "ssh".to_string(),
                service_type: ServiceType::Ssh,
                token: "token2".to_string(),
                socks: None,
                ssh: Some(SshConfig::default()),
            },
        ];

        let socks = services.get_service("socks");
        assert!(socks.is_some());
        assert_eq!(socks.unwrap().name, "socks");

        let ssh = services.get_service("ssh");
        assert!(ssh.is_some());
        assert_eq!(ssh.unwrap().service_type, ServiceType::Ssh);

        let missing = services.get_service("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_service_list_filter_by_type() {
        let services = vec![
            ServiceConfig {
                name: "socks1".to_string(),
                service_type: ServiceType::Socks5,
                token: "token1".to_string(),
                socks: Some(SocksConfig::default()),
                ssh: None,
            },
            ServiceConfig {
                name: "ssh1".to_string(),
                service_type: ServiceType::Ssh,
                token: "token2".to_string(),
                socks: None,
                ssh: Some(SshConfig::default()),
            },
            ServiceConfig {
                name: "socks2".to_string(),
                service_type: ServiceType::Socks5,
                token: "token3".to_string(),
                socks: Some(SocksConfig::default()),
                ssh: None,
            },
        ];

        let socks_services = services.socks_services();
        assert_eq!(socks_services.len(), 2);

        let ssh_services = services.ssh_services();
        assert_eq!(ssh_services.len(), 1);
    }
}
