//! Client module for Sockrats
//!
//! This module contains the main client logic for connecting to
//! the rathole server and handling SOCKS5 requests.

#[allow(clippy::module_inception)]
mod client;
mod control_channel;
mod data_channel;

pub use client::Client;
pub use control_channel::ControlChannel;
pub use data_channel::run_data_channel;

use crate::config::Config;
use crate::transport::{NoiseTransport, TcpTransport};
use anyhow::Result;
use tokio::sync::broadcast;

/// Run the client with the given configuration
pub async fn run_client(config: Config, shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
    let client_config = config.client;

    // Select transport based on configuration
    match client_config.transport.transport_type {
        crate::config::TransportType::Tcp => {
            let client = Client::<TcpTransport>::new(client_config).await?;
            client.run(shutdown_rx).await
        }
        crate::config::TransportType::Noise => {
            let client = Client::<NoiseTransport>::new(client_config).await?;
            client.run(shutdown_rx).await
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_exports() {
        // This test just verifies the module structure is correct
        // by ensuring the exports compile
    }
}
