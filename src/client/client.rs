//! Main client structure
//!
//! Manages the client lifecycle and transport.

use super::control_channel::ControlChannel;
use crate::config::ClientConfig;
use crate::transport::Transport;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

/// Main SocksRat client
pub struct Client<T: Transport> {
    /// Client configuration
    config: ClientConfig,
    /// Transport layer
    transport: Arc<T>,
}

impl<T: Transport + 'static> Client<T> {
    /// Create a new client with the given configuration
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let transport = Arc::new(T::new(&config.transport)?);
        Ok(Client { config, transport })
    }

    /// Run the client until shutdown
    pub async fn run(self, mut shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
        info!("Starting SocksRat client");
        info!("Remote server: {}", self.config.remote_addr);
        info!("Service name: {}", self.config.service_name);

        let control_channel = ControlChannel::new(self.config.clone(), self.transport.clone());

        tokio::select! {
            result = control_channel.run() => {
                if let Err(e) = result {
                    error!("Control channel error: {:#}", e);
                    return Err(e);
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping client");
            }
        }

        info!("Client stopped");
        Ok(())
    }

    /// Get a reference to the transport
    pub fn transport(&self) -> &Arc<T> {
        &self.transport
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SocksConfig, TransportConfig};

    fn create_test_config() -> ClientConfig {
        ClientConfig {
            remote_addr: "127.0.0.1:2333".to_string(),
            service_name: "test-socks".to_string(),
            token: "test-token".to_string(),
            transport: TransportConfig::default(),
            heartbeat_timeout: 40,
            socks: SocksConfig::default(),
            pool: Default::default(),
        }
    }

    #[test]
    fn test_config_getters() {
        let config = create_test_config();
        assert_eq!(config.remote_addr, "127.0.0.1:2333");
        assert_eq!(config.service_name, "test-socks");
        assert_eq!(config.token, "test-token");
    }
}
