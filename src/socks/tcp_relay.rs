//! TCP relay for SOCKS5 CONNECT command
//!
//! Handles TCP CONNECT requests by establishing a connection to the target
//! and relaying data bidirectionally.

use crate::config::SocksConfig;
use crate::socks::command::build_reply;
use crate::socks::consts::*;
use crate::socks::types::TargetAddr;
use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

/// Handle TCP CONNECT command
///
/// This function:
/// 1. Resolves the target address
/// 2. Establishes a TCP connection to the target
/// 3. Sends a success reply
/// 4. Relays data bidirectionally between client and target
///
/// # Arguments
///
/// * `client_stream` - The client stream (from tunnel)
/// * `target_addr` - The target address to connect to
/// * `config` - SOCKS5 configuration
pub async fn handle_tcp_connect<S>(
    mut client_stream: S,
    target_addr: TargetAddr,
    config: &SocksConfig,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let timeout = Duration::from_secs(config.request_timeout);

    // Resolve address
    let socket_addr = target_addr.resolve().await
        .with_context(|| format!("Failed to resolve address: {}", target_addr))?;

    debug!("Connecting to target: {}", socket_addr);

    // Connect to target with timeout
    let target_stream = match tokio::time::timeout(timeout, TcpStream::connect(socket_addr)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            error!("Failed to connect to {}: {}", socket_addr, e);
            let reply_code = io_error_to_reply_code(&e);
            build_reply(&mut client_stream, reply_code, None).await?;
            return Err(e.into());
        }
        Err(_) => {
            error!("Connection timeout to {}", socket_addr);
            build_reply(&mut client_stream, SOCKS5_REPLY_HOST_UNREACHABLE, None).await?;
            anyhow::bail!("Connection timeout");
        }
    };

    // Get local address for reply
    let local_addr = target_stream.local_addr().ok();

    // Send success reply
    build_reply(&mut client_stream, SOCKS5_REPLY_SUCCEEDED, local_addr).await?;

    info!("SOCKS5 tunnel established to {}", socket_addr);

    // Perform bidirectional relay
    relay_tcp(client_stream, target_stream).await
}

/// Relay data bidirectionally between two streams
///
/// This function copies data in both directions concurrently and
/// returns when either direction encounters an error or EOF.
pub async fn relay_tcp<A, B>(a: A, b: B) -> Result<()>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (mut a_read, mut a_write) = tokio::io::split(a);
    let (mut b_read, mut b_write) = tokio::io::split(b);

    let a_to_b = tokio::io::copy(&mut a_read, &mut b_write);
    let b_to_a = tokio::io::copy(&mut b_read, &mut a_write);

    tokio::select! {
        result = a_to_b => {
            match result {
                Ok(bytes) => debug!("A->B finished: {} bytes", bytes),
                Err(e) => debug!("A->B error: {}", e),
            }
        }
        result = b_to_a => {
            match result {
                Ok(bytes) => debug!("B->A finished: {} bytes", bytes),
                Err(e) => debug!("B->A error: {}", e),
            }
        }
    }

    Ok(())
}

/// Convert IO error to SOCKS5 reply code
fn io_error_to_reply_code(error: &std::io::Error) -> u8 {
    match error.kind() {
        std::io::ErrorKind::ConnectionRefused => SOCKS5_REPLY_CONNECTION_REFUSED,
        std::io::ErrorKind::TimedOut => SOCKS5_REPLY_HOST_UNREACHABLE,
        std::io::ErrorKind::AddrNotAvailable => SOCKS5_REPLY_HOST_UNREACHABLE,
        std::io::ErrorKind::PermissionDenied => SOCKS5_REPLY_CONNECTION_NOT_ALLOWED,
        _ => SOCKS5_REPLY_GENERAL_FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_io_error_to_reply_code() {
        let cases = vec![
            (io::ErrorKind::ConnectionRefused, SOCKS5_REPLY_CONNECTION_REFUSED),
            (io::ErrorKind::TimedOut, SOCKS5_REPLY_HOST_UNREACHABLE),
            (io::ErrorKind::AddrNotAvailable, SOCKS5_REPLY_HOST_UNREACHABLE),
            (io::ErrorKind::PermissionDenied, SOCKS5_REPLY_CONNECTION_NOT_ALLOWED),
            (io::ErrorKind::Other, SOCKS5_REPLY_GENERAL_FAILURE),
            (io::ErrorKind::NotFound, SOCKS5_REPLY_GENERAL_FAILURE),
        ];

        for (error_kind, expected_code) in cases {
            let error = io::Error::new(error_kind, "test error");
            assert_eq!(io_error_to_reply_code(&error), expected_code);
        }
    }

    #[tokio::test]
    async fn test_relay_tcp_echo() {
        use tokio::io::{duplex, AsyncWriteExt};

        // Create two pairs of duplex streams
        let (mut client_a, server_a) = duplex(1024);
        let (mut client_b, server_b) = duplex(1024);

        // Spawn the relay
        let relay_handle = tokio::spawn(async move {
            relay_tcp(server_a, server_b).await
        });

        // Write to client_a, should be readable from client_b
        client_a.write_all(b"hello from a").await.unwrap();

        // Write to client_b, should be readable from client_a
        client_b.write_all(b"hello from b").await.unwrap();

        // Allow some time for relay
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Close one side to end the relay
        drop(client_a);
        drop(client_b);

        // Wait for relay to finish
        let _ = tokio::time::timeout(Duration::from_millis(100), relay_handle).await;
    }
}
