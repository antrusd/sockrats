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
#[cfg(feature = "noise")]
use crate::transport::NoiseTransport;
use crate::transport::TcpTransport;
#[cfg(feature = "wireguard")]
use crate::transport::WireguardTransport;
use anyhow::Result;
use tokio::sync::broadcast;

/// Run the client with the given configuration
pub async fn run_client(config: Config, shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
    let mut client_config = config.client;

    // Check WireGuard tunnel (separate layer, not a transport type)
    #[cfg(feature = "wireguard")]
    if client_config.wireguard_enabled() {
        // Validate: WireGuard requires transport type = tcp
        if client_config.transport.transport_type != crate::config::TransportType::Tcp {
            anyhow::bail!(
                "WireGuard tunnel requires transport type 'tcp', got '{:?}'. \
                 Noise encryption is redundant when using WireGuard.",
                client_config.transport.transport_type
            );
        }

        // Copy the WireGuard config into TransportConfig so that
        // Transport::new() can access it.
        client_config.transport.wireguard = client_config.wireguard.clone();

        let client = Client::<WireguardTransport>::new(client_config).await?;
        return client.run(shutdown_rx).await;
    }

    // Existing transport selection (unchanged when WireGuard disabled)
    match client_config.transport.transport_type {
        crate::config::TransportType::Tcp => {
            let client = Client::<TcpTransport>::new(client_config).await?;
            client.run(shutdown_rx).await
        }
        #[cfg(feature = "noise")]
        crate::config::TransportType::Noise => {
            let client = Client::<NoiseTransport>::new(client_config).await?;
            client.run(shutdown_rx).await
        }
        #[cfg(not(feature = "noise"))]
        crate::config::TransportType::Noise => {
            anyhow::bail!("Noise transport is not enabled. Recompile with --features noise")
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
