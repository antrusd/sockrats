//! Configuration module for Sockrats
//!
//! This module provides configuration types and parsing for the client.

mod client;
mod pool;
mod transport;

pub use client::{ClientConfig, Config, ServiceConfig, ServiceListExt, ServiceType, SocksConfig};
pub use pool::PoolConfig;
pub use transport::{NoiseConfig, TcpConfig, TransportConfig, TransportType};

use anyhow::{Context, Result};
use std::path::Path;

/// Load configuration from a TOML file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let content = std::fs::read_to_string(path.as_ref())
        .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;

    parse_config(&content)
}

/// Parse configuration from a TOML string
pub fn parse_config(content: &str) -> Result<Config> {
    toml::from_str(content).with_context(|| "Failed to parse configuration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let config_str = r#"
[client]
remote_addr = "server.example.com:2333"
service_name = "socks5"
token = "secret-token"
"#;

        let config = parse_config(config_str).unwrap();
        assert_eq!(config.client.remote_addr, "server.example.com:2333");
        assert_eq!(config.client.service_name, "socks5");
        assert_eq!(config.client.token, "secret-token");
    }

    #[test]
    fn test_parse_full_config() {
        let config_str = r#"
[client]
remote_addr = "server.example.com:2333"
service_name = "socks5"
token = "secret-token"
heartbeat_timeout = 60

[client.transport]
type = "tcp"

[client.transport.tcp]
nodelay = true
keepalive_secs = 30
keepalive_interval = 10

[client.socks]
auth_required = true
username = "user"
password = "pass"
allow_udp = true
dns_resolve = true
request_timeout = 15

[client.pool]
min_tcp_channels = 4
max_tcp_channels = 20
min_udp_channels = 2
max_udp_channels = 10
"#;

        let config = parse_config(config_str).unwrap();
        assert_eq!(config.client.heartbeat_timeout, 60);
        assert!(config.client.socks.auth_required);
        assert_eq!(config.client.socks.username, Some("user".to_string()));
        assert_eq!(config.client.pool.min_tcp_channels, 4);
    }
}
