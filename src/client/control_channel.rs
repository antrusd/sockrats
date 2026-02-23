//! Control channel management
//!
//! Handles the control channel connection to the rathole server.
//! Each control channel manages one service and spawns data channels
//! that are routed to the appropriate [`ServiceHandler`].

use super::data_channel::run_data_channel;
use crate::config::ClientConfig;
use crate::protocol::{
    read_ack, read_control_cmd, read_hello, write_auth, write_hello, Ack, Auth, ControlChannelCmd,
    Digest, Hello,
};
use crate::services::ServiceHandler;
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{bail, Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, error, info, warn};

/// Control channel for managing the connection to the rathole server
pub struct ControlChannel<T: Transport> {
    /// Client configuration
    config: ClientConfig,
    /// Transport layer
    transport: Arc<T>,
    /// Service handler for data channels spawned by this control channel
    handler: Arc<dyn ServiceHandler>,
}

impl<T: Transport + 'static> ControlChannel<T> {
    /// Create a new control channel with a specific service handler
    pub fn new(config: ClientConfig, transport: Arc<T>, handler: Arc<dyn ServiceHandler>) -> Self {
        ControlChannel {
            config,
            transport,
            handler,
        }
    }

    /// Run the control channel with automatic reconnection
    pub async fn run(&self) -> Result<()> {
        let mut retry_count = 0;
        let max_retries = 10;
        let base_delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);

        loop {
            match self.run_once().await {
                Ok(_) => {
                    info!("Control channel closed normally");
                    break;
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count > max_retries {
                        error!("Max retries exceeded, giving up");
                        return Err(e);
                    }

                    let delay =
                        std::cmp::min(base_delay * 2u32.pow((retry_count - 1) as u32), max_delay);

                    warn!(
                        "Control channel error: {:#}. Reconnecting in {:?}... (attempt {}/{})",
                        e, delay, retry_count, max_retries
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }

        Ok(())
    }

    /// Run a single control channel session
    async fn run_once(&self) -> Result<()> {
        let remote_addr = AddrMaybeCached::new(&self.config.remote_addr);

        info!("Connecting to server: {}", self.config.remote_addr);

        let mut conn = self
            .transport
            .connect(&remote_addr)
            .await
            .context("Failed to connect to server")?;

        T::hint(&conn, SocketOpts::for_control_channel());

        // Perform handshake
        let session_key = self
            .do_handshake(&mut conn)
            .await
            .context("Handshake failed")?;

        info!("Control channel established");

        // Listen for commands
        self.handle_commands(conn, session_key, remote_addr).await
    }

    /// Perform the control channel handshake
    async fn do_handshake<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        conn: &mut S,
    ) -> Result<Digest> {
        // Send control channel hello
        let hello = Hello::control_channel(&self.config.service_name);
        write_hello(conn, &hello).await?;

        debug!("Sent control channel hello");

        // Read server's response (contains nonce)
        let server_hello = read_hello(conn).await?;
        let nonce = match server_hello {
            Hello::ControlChannelHello(_, n) => n,
            _ => bail!("Unexpected hello type from server"),
        };

        debug!("Received server nonce");

        // Create and send auth
        let auth = Auth::new(&self.config.token, &nonce);
        let session_key = auth.0;
        write_auth(conn, &auth).await?;

        debug!("Sent authentication");

        // Read ack
        let ack = read_ack(conn).await?;
        match ack {
            Ack::Ok => {
                debug!("Authentication successful");
                Ok(session_key)
            }
            Ack::ServiceNotExist => {
                bail!(
                    "Service '{}' does not exist on server",
                    self.config.service_name
                )
            }
            Ack::AuthFailed => bail!("Authentication failed - incorrect token"),
        }
    }

    /// Handle commands from the server
    async fn handle_commands<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        mut conn: S,
        session_key: Digest,
        remote_addr: AddrMaybeCached,
    ) -> Result<()> {
        let heartbeat_timeout = Duration::from_secs(self.config.heartbeat_timeout);

        info!(
            "Using service handler: {} (type: {})",
            self.config.service_name,
            self.handler.service_type()
        );

        loop {
            tokio::select! {
                cmd_result = read_control_cmd(&mut conn) => {
                    let cmd = cmd_result.context("Failed to read control command")?;

                    match cmd {
                        ControlChannelCmd::CreateDataChannel => {
                            debug!("Received CreateDataChannel command");

                            // Spawn data channel handler with the service handler
                            let transport = self.transport.clone();
                            let addr = remote_addr.clone();
                            let key = session_key;
                            let handler = self.handler.clone();

                            tokio::spawn(async move {
                                if let Err(e) = run_data_channel(
                                    transport,
                                    addr,
                                    key,
                                    handler,
                                ).await {
                                    warn!("Data channel error: {:#}", e);
                                }
                            });
                        }
                        ControlChannelCmd::HeartBeat => {
                            debug!("Received heartbeat");
                        }
                    }
                }
                _ = tokio::time::sleep(heartbeat_timeout) => {
                    bail!("Heartbeat timeout - no command received in {:?}", heartbeat_timeout);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SocksConfig, TransportConfig};
    use crate::services::ssh::SshConfig;
    use crate::services::{Socks5ServiceHandler, SshServiceHandler};

    fn create_test_config() -> ClientConfig {
        ClientConfig {
            remote_addr: "127.0.0.1:2333".to_string(),
            service_name: "test".to_string(),
            token: "secret".to_string(),
            transport: TransportConfig::default(),
            heartbeat_timeout: 40,
            socks: SocksConfig::default(),
            ssh: SshConfig::default(),
            pool: Default::default(),
            services: Vec::new(),
        }
    }

    #[test]
    fn test_control_channel_config() {
        let config = create_test_config();
        assert_eq!(config.heartbeat_timeout, 40);
        assert_eq!(config.service_name, "test");
    }

    #[test]
    fn test_control_channel_socks5_handler() {
        let handler = Socks5ServiceHandler::new(SocksConfig::default());
        assert_eq!(handler.service_type(), "socks5");
    }

    #[test]
    fn test_control_channel_ssh_handler() {
        let handler = SshServiceHandler::new(SshConfig::default());
        assert_eq!(handler.service_type(), "ssh");
    }
}
