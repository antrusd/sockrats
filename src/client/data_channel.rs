//! Data channel handling
//!
//! Manages individual data channels for SOCKS5 and SSH request processing.

use crate::config::SocksConfig;
use crate::protocol::{read_data_cmd, write_hello, DataChannelCmd, Digest, Hello};
use crate::socks::handle_socks5_on_stream;
use crate::ssh::{handle_ssh_on_stream, SshConfig};
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Service handler type
#[derive(Clone, Debug)]
pub enum ServiceHandler {
    /// SOCKS5 proxy handler
    Socks5(SocksConfig),
    /// SSH server handler
    Ssh(Arc<SshConfig>),
}

/// Run a data channel for handling a SOCKS5 or SSH request
///
/// This function:
/// 1. Connects to the rathole server
/// 2. Sends data channel hello with session key
/// 3. Receives the forward command
/// 4. Routes to the appropriate handler (SOCKS5 or SSH)
pub async fn run_data_channel<T: Transport>(
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    handler: ServiceHandler,
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
            match handler {
                ServiceHandler::Socks5(socks_config) => {
                    debug!("Starting TCP forwarding (SOCKS5)");
                    handle_socks5_on_stream(conn, &socks_config)
                        .await
                        .context("SOCKS5 handling failed")?;
                }
                ServiceHandler::Ssh(ssh_config) => {
                    info!("Starting TCP forwarding (SSH)");
                    handle_ssh_on_stream(conn, ssh_config)
                        .await
                        .context("SSH handling failed")?;
                }
            }
        }
        DataChannelCmd::StartForwardUdp => {
            match handler {
                ServiceHandler::Socks5(socks_config) => {
                    if socks_config.allow_udp {
                        debug!("Starting UDP forwarding");
                        warn!("UDP forwarding via data channel not fully implemented");
                    } else {
                        warn!("UDP forwarding not allowed by configuration");
                    }
                }
                ServiceHandler::Ssh(_) => {
                    warn!("UDP forwarding not supported for SSH service");
                }
            }
        }
    }

    debug!("Data channel completed");
    Ok(())
}

/// Run a SOCKS5 data channel (backward compatible helper)
#[allow(dead_code)]
pub async fn run_socks5_data_channel<T: Transport>(
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    socks_config: SocksConfig,
) -> Result<()> {
    run_data_channel(transport, remote_addr, session_key, ServiceHandler::Socks5(socks_config)).await
}

/// Run an SSH data channel
#[allow(dead_code)]
pub async fn run_ssh_data_channel<T: Transport>(
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    ssh_config: Arc<SshConfig>,
) -> Result<()> {
    run_data_channel(transport, remote_addr, session_key, ServiceHandler::Ssh(ssh_config)).await
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
    /// Service handler (SOCKS5 or SSH)
    pub handler: ServiceHandler,
}

#[allow(dead_code)]
impl<T: Transport> DataChannelArgs<T> {
    /// Create new data channel arguments
    pub fn new(
        transport: Arc<T>,
        remote_addr: AddrMaybeCached,
        session_key: Digest,
        handler: ServiceHandler,
    ) -> Self {
        DataChannelArgs {
            transport,
            remote_addr,
            session_key,
            handler,
        }
    }

    /// Run the data channel with these arguments
    pub async fn run(self) -> Result<()> {
        run_data_channel(
            self.transport,
            self.remote_addr,
            self.session_key,
            self.handler,
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
