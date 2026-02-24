//! Data channel handling
//!
//! Manages individual data channels for service request processing.
//! Routes incoming connections to the appropriate service handler
//! (SOCKS5, SSH, etc.) via the [`ServiceHandler`] trait.

use crate::protocol::{read_data_cmd, write_hello, DataChannelCmd, Digest, Hello};
use crate::services::ServiceHandler;
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::debug;

/// Run a data channel for handling a service request
///
/// This function:
/// 1. Connects to the rathole server
/// 2. Sends data channel hello with session key
/// 3. Receives the forward command
/// 4. Routes to the appropriate handler via the [`ServiceHandler`] trait
pub async fn run_data_channel<T: Transport>(
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    handler: Arc<dyn ServiceHandler>,
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
            debug!("Starting TCP forwarding ({})", handler.service_type());
            handler
                .handle_tcp_stream(Box::new(conn))
                .await
                .with_context(|| format!("{} TCP handling failed", handler.service_type()))?;
        }
        DataChannelCmd::StartForwardUdp => {
            debug!("Starting UDP forwarding ({})", handler.service_type());
            handler
                .handle_udp_stream(Box::new(conn))
                .await
                .with_context(|| format!("{} UDP handling failed", handler.service_type()))?;
        }
    }

    debug!("Data channel completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::services::{ServiceHandler, StreamDyn};

    // A minimal mock service handler for data channel tests
    #[derive(Debug)]
    struct MockHandler;

    #[async_trait::async_trait]
    impl ServiceHandler for MockHandler {
        fn service_type(&self) -> &str {
            "mock"
        }

        async fn handle_tcp_stream(&self, _stream: Box<dyn StreamDyn>) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_mock_handler_service_type() {
        let handler = MockHandler;
        assert_eq!(handler.service_type(), "mock");
    }

    #[test]
    fn test_mock_handler_is_healthy() {
        let handler = MockHandler;
        assert!(handler.is_healthy());
    }

    #[test]
    fn test_mock_handler_validate() {
        let handler = MockHandler;
        assert!(handler.validate().is_ok());
    }
}
