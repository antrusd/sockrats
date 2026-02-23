//! SOCKS5 service module for Sockrats
//!
//! This module implements the SOCKS5 protocol for handling proxy requests
//! through the rathole tunnel. It processes SOCKS5 requests directly on
//! the tunnel stream without binding to any local network interface.

mod auth;
mod command;
mod consts;
mod handler;
mod tcp_relay;
mod types;
mod udp;

pub use auth::{authenticate, AuthMethod};
pub use command::{build_reply, parse_command};
pub use consts::*;
pub use handler::handle_socks5_on_stream;
pub use tcp_relay::relay_tcp;
pub use types::{SocksCommand, TargetAddr};
pub use udp::{handle_udp_associate, UdpRelay};

use crate::config::SocksConfig;
use crate::services::{ServiceHandler, StreamDyn};
use anyhow::Result;

/// SOCKS5 service handler implementing the [`ServiceHandler`] trait.
///
/// Wraps the existing SOCKS5 protocol implementation to conform to the
/// service handler interface, allowing it to be registered in the
/// [`ServiceRegistry`](crate::services::ServiceRegistry).
#[derive(Debug, Clone)]
pub struct Socks5ServiceHandler {
    config: SocksConfig,
}

impl Socks5ServiceHandler {
    /// Create a new SOCKS5 service handler with the given configuration.
    pub fn new(config: SocksConfig) -> Self {
        Self { config }
    }

    /// Get a reference to the SOCKS5 configuration.
    pub fn config(&self) -> &SocksConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl ServiceHandler for Socks5ServiceHandler {
    fn service_type(&self) -> &str {
        "socks5"
    }

    async fn handle_tcp_stream(&self, stream: Box<dyn StreamDyn>) -> Result<()> {
        handle_socks5_on_stream(stream, &self.config).await
    }

    async fn handle_udp_stream(&self, _stream: Box<dyn StreamDyn>) -> Result<()> {
        if self.config.allow_udp {
            tracing::warn!("UDP ASSOCIATE via data channel not fully implemented");
            Ok(())
        } else {
            anyhow::bail!("UDP not allowed by SOCKS5 configuration")
        }
    }

    fn validate(&self) -> Result<()> {
        self.config.validate().map_err(|e| anyhow::anyhow!(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks5_service_handler_new() {
        let config = SocksConfig::default();
        let handler = Socks5ServiceHandler::new(config);
        assert_eq!(handler.service_type(), "socks5");
    }

    #[test]
    fn test_socks5_service_handler_config() {
        let config = SocksConfig {
            auth_required: true,
            ..Default::default()
        };
        let handler = Socks5ServiceHandler::new(config);
        assert!(handler.config().auth_required);
    }

    #[test]
    fn test_socks5_service_handler_is_healthy() {
        let handler = Socks5ServiceHandler::new(SocksConfig::default());
        assert!(handler.is_healthy());
    }

    #[test]
    fn test_socks5_service_handler_validate() {
        let handler = Socks5ServiceHandler::new(SocksConfig::default());
        assert!(handler.validate().is_ok());
    }

    #[test]
    fn test_socks5_service_handler_debug() {
        let handler = Socks5ServiceHandler::new(SocksConfig::default());
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("Socks5ServiceHandler"));
    }

    #[test]
    fn test_socks5_service_handler_clone() {
        let handler = Socks5ServiceHandler::new(SocksConfig::default());
        let cloned = handler.clone();
        assert_eq!(cloned.service_type(), "socks5");
    }
}
