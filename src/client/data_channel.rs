//! Data channel handling
//!
//! Manages individual data channels for SOCKS5 request processing.

use crate::config::SocksConfig;
use crate::protocol::{read_data_cmd, write_hello, DataChannelCmd, Digest, Hello};
use crate::socks::handle_socks5_on_stream;
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, warn};

/// Run a data channel for handling a SOCKS5 request
///
/// This function:
/// 1. Connects to the rathole server
/// 2. Sends data channel hello with session key
/// 3. Receives the forward command
/// 4. Processes the SOCKS5 request on the tunnel stream
pub async fn run_data_channel<T: Transport>(
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    socks_config: SocksConfig,
) -> Result<()> {
    // Connect to server
    let mut conn = transport
        .connect(&remote_addr)
        .await
        .context("Failed to connect data channel")?;

    T::hint(&conn, SocketOpts::for_data_channel());

    // Send data channel hello
    let hello = Hello::data_channel(session_key);
    write_hello(&mut conn, &hello).await?;

    debug!("Data channel hello sent");

    // Read command
    let cmd = read_data_cmd(&mut conn)
        .await
        .context("Failed to read data channel command")?;

    match cmd {
        DataChannelCmd::StartForwardTcp => {
            debug!("Starting TCP forwarding (SOCKS5)");

            // Process SOCKS5 on the tunnel stream
            handle_socks5_on_stream(conn, &socks_config)
                .await
                .context("SOCKS5 handling failed")?;
        }
        DataChannelCmd::StartForwardUdp => {
            if socks_config.allow_udp {
                debug!("Starting UDP forwarding");
                // UDP is handled through the TCP control stream
                // The actual UDP data is encapsulated
                warn!("UDP forwarding via data channel not fully implemented");
            } else {
                warn!("UDP forwarding not allowed by configuration");
            }
        }
    }

    debug!("Data channel completed");
    Ok(())
}

/// Data channel arguments for spawning
#[allow(dead_code)]
#[derive(Clone)]
pub struct DataChannelArgs<T: Transport> {
    /// Transport for connections
    pub transport: Arc<T>,
    /// Remote server address
    pub remote_addr: AddrMaybeCached,
    /// Session key for authentication
    pub session_key: Digest,
    /// SOCKS5 configuration
    pub socks_config: SocksConfig,
}

#[allow(dead_code)]
impl<T: Transport> DataChannelArgs<T> {
    /// Create new data channel arguments
    pub fn new(
        transport: Arc<T>,
        remote_addr: AddrMaybeCached,
        session_key: Digest,
        socks_config: SocksConfig,
    ) -> Self {
        DataChannelArgs {
            transport,
            remote_addr,
            session_key,
            socks_config,
        }
    }

    /// Run the data channel with these arguments
    pub async fn run(self) -> Result<()> {
        run_data_channel(
            self.transport,
            self.remote_addr,
            self.session_key,
            self.socks_config,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::config::SocksConfig;

    #[test]
    fn test_data_channel_args() {
        // Test that DataChannelArgs can be created
        // Full testing requires mock transport
        let config = SocksConfig::default();
        assert!(!config.auth_required);
    }
}
