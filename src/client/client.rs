//! Main client structure
//!
//! Manages the client lifecycle and transport.

use super::control_channel::ControlChannel;
use crate::config::{ClientConfig, ServiceConfig};
use crate::transport::Transport;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

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

        // Determine which services to run
        let services = self.config.effective_services();

        if services.is_empty() {
            // Legacy mode: use service_name from [client] section
            warn!("No services configured, using legacy single-service mode");
            info!("Service name: {}", self.config.service_name);

            let control_channel =
                ControlChannel::new(self.config.clone(), self.transport.clone());

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
        } else {
            // Multi-service mode: spawn a control channel for each service
            info!("Running {} services", services.len());
            for service in &services {
                info!("  - {} (type: {:?})", service.name, service.service_type);
            }

            let mut handles = Vec::new();

            for service in services {
                let config = self.create_service_config(&service);
                let transport = self.transport.clone();
                let shutdown_rx = shutdown_rx.resubscribe();

                let handle = tokio::spawn(async move {
                    let control_channel = ControlChannel::new(config, transport);
                    Self::run_service_loop(control_channel, shutdown_rx).await
                });
                handles.push(handle);
            }

            // Wait for shutdown or any service to fail
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping all services");
                }
                result = futures::future::select_all(handles.iter_mut().map(|h| Box::pin(h))) => {
                    if let (Ok(Err(e)), _, _) = result {
                        error!("A service control channel failed: {:#}", e);
                    }
                }
            }
        }

        info!("Client stopped");
        Ok(())
    }

    /// Run a service control channel loop with shutdown handling
    async fn run_service_loop(
        control_channel: ControlChannel<T>,
        mut shutdown_rx: broadcast::Receiver<bool>,
    ) -> Result<()> {
        tokio::select! {
            result = control_channel.run() => {
                result
            }
            _ = shutdown_rx.recv() => {
                Ok(())
            }
        }
    }

    /// Create a ClientConfig for a specific service
    fn create_service_config(&self, service: &ServiceConfig) -> ClientConfig {
        let mut config = self.config.clone();
        config.service_name = service.name.clone();
        config.token = service.token.clone();

        // Apply service-specific SSH config if present
        if let Some(ssh) = &service.ssh {
            config.ssh = ssh.clone();
        }

        // Apply service-specific SOCKS config if present
        if let Some(socks) = &service.socks {
            config.socks = socks.clone();
        }

        config
    }

    /// Get a reference to the transport
    #[allow(dead_code)]
    pub fn transport(&self) -> &Arc<T> {
        &self.transport
    }

    /// Get a reference to the configuration
    #[allow(dead_code)]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SocksConfig, TransportConfig};
    use crate::ssh::SshConfig;

    fn create_test_config() -> ClientConfig {
        ClientConfig {
            remote_addr: "127.0.0.1:2333".to_string(),
            service_name: "test-socks".to_string(),
            token: "test-token".to_string(),
            transport: TransportConfig::default(),
            heartbeat_timeout: 40,
            socks: SocksConfig::default(),
            ssh: SshConfig::default(),
            pool: Default::default(),
            services: Vec::new(),
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
