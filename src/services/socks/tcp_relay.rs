//! TCP relay for SOCKS5 CONNECT command
//!
//! Handles TCP CONNECT requests by establishing a connection to the target
//! and relaying data bidirectionally.

use crate::config::SocksConfig;
use crate::services::socks::command::build_reply;
use crate::services::socks::consts::*;
use crate::services::socks::types::TargetAddr;
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
    let socket_addr = target_addr
        .resolve()
        .await
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
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_io_error_to_reply_code() {
        let cases = vec![
            (
                io::ErrorKind::ConnectionRefused,
                SOCKS5_REPLY_CONNECTION_REFUSED,
            ),
            (io::ErrorKind::TimedOut, SOCKS5_REPLY_HOST_UNREACHABLE),
            (
                io::ErrorKind::AddrNotAvailable,
                SOCKS5_REPLY_HOST_UNREACHABLE,
            ),
            (
                io::ErrorKind::PermissionDenied,
                SOCKS5_REPLY_CONNECTION_NOT_ALLOWED,
            ),
            (io::ErrorKind::Other, SOCKS5_REPLY_GENERAL_FAILURE),
            (io::ErrorKind::NotFound, SOCKS5_REPLY_GENERAL_FAILURE),
        ];

        for (error_kind, expected_code) in cases {
            let error = io::Error::new(error_kind, "test error");
            assert_eq!(io_error_to_reply_code(&error), expected_code);
        }
    }

    #[test]
    fn test_io_error_to_reply_code_all_variants() {
        assert_eq!(
            io_error_to_reply_code(&io::Error::from(io::ErrorKind::ConnectionRefused)),
            SOCKS5_REPLY_CONNECTION_REFUSED
        );
        assert_eq!(
            io_error_to_reply_code(&io::Error::from(io::ErrorKind::TimedOut)),
            SOCKS5_REPLY_HOST_UNREACHABLE
        );
        assert_eq!(
            io_error_to_reply_code(&io::Error::from(io::ErrorKind::AddrNotAvailable)),
            SOCKS5_REPLY_HOST_UNREACHABLE
        );
        assert_eq!(
            io_error_to_reply_code(&io::Error::from(io::ErrorKind::PermissionDenied)),
            SOCKS5_REPLY_CONNECTION_NOT_ALLOWED
        );
        assert_eq!(
            io_error_to_reply_code(&io::Error::from(io::ErrorKind::WouldBlock)),
            SOCKS5_REPLY_GENERAL_FAILURE
        );
    }

    #[tokio::test]
    async fn test_relay_tcp_echo() {
        // Create two pairs of duplex streams
        let (mut client_a, server_a) = duplex(1024);
        let (mut client_b, server_b) = duplex(1024);

        // Spawn the relay
        let relay_handle = tokio::spawn(async move { relay_tcp(server_a, server_b).await });

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

    #[tokio::test]
    async fn test_relay_tcp_bidirectional() {
        let (mut client_a, server_a) = duplex(1024);
        let (mut client_b, server_b) = duplex(1024);

        let relay_handle = tokio::spawn(async move { relay_tcp(server_a, server_b).await });

        // Write from A to B
        client_a.write_all(b"message A->B").await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        let mut buf_b = vec![0u8; 12];
        client_b.read_exact(&mut buf_b).await.unwrap();
        assert_eq!(&buf_b, b"message A->B");

        // Write from B to A
        client_b.write_all(b"message B->A").await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        let mut buf_a = vec![0u8; 12];
        client_a.read_exact(&mut buf_a).await.unwrap();
        assert_eq!(&buf_a, b"message B->A");

        drop(client_a);
        drop(client_b);

        let _ = tokio::time::timeout(Duration::from_millis(100), relay_handle).await;
    }

    #[tokio::test]
    async fn test_relay_tcp_large_data() {
        let (mut client_a, server_a) = duplex(65536);
        let (mut client_b, server_b) = duplex(65536);

        let relay_handle = tokio::spawn(async move { relay_tcp(server_a, server_b).await });

        // Send large data
        let large_data = vec![0xAB; 50000];
        client_a.write_all(&large_data).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut received = vec![0u8; 50000];
        client_b.read_exact(&mut received).await.unwrap();
        assert_eq!(received, large_data);

        drop(client_a);
        drop(client_b);

        let _ = tokio::time::timeout(Duration::from_millis(100), relay_handle).await;
    }

    #[tokio::test]
    async fn test_relay_tcp_closes_on_eof() {
        let (mut client_a, server_a) = duplex(1024);
        let (client_b, server_b) = duplex(1024);

        let relay_handle = tokio::spawn(async move { relay_tcp(server_a, server_b).await });

        // Send some data then close
        client_a.write_all(b"data").await.unwrap();
        drop(client_a);
        drop(client_b);

        // Relay should finish
        let result = tokio::time::timeout(Duration::from_millis(100), relay_handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_relay_tcp_empty_transfer() {
        let (client_a, server_a) = duplex(1024);
        let (client_b, server_b) = duplex(1024);

        let relay_handle = tokio::spawn(async move { relay_tcp(server_a, server_b).await });

        // Close immediately without writing
        drop(client_a);
        drop(client_b);

        // Relay should finish quickly
        let result = tokio::time::timeout(Duration::from_millis(100), relay_handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_tcp_connect_invalid_address() {
        let (client, _server) = duplex(1024);

        let config = SocksConfig {
            username: None,
            password: None,
            auth_required: false,
            dns_resolve: false,
            allow_udp: false,
            request_timeout: 1,
        };

        // Try to connect to an invalid port (0)
        let target = TargetAddr::Ip("0.0.0.0:0".parse().unwrap());
        let result = handle_tcp_connect(client, target, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_tcp_connect_connection_refused() {
        let (client, _server) = duplex(1024);

        let config = SocksConfig {
            username: None,
            password: None,
            auth_required: false,
            dns_resolve: false,
            allow_udp: false,
            request_timeout: 1,
        };

        // Try to connect to a port that's not listening
        let target = TargetAddr::Ip("127.0.0.1:9".parse().unwrap());
        let result = handle_tcp_connect(client, target, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_tcp_connect_unresolvable_domain() {
        let (client, _server) = duplex(1024);

        let config = SocksConfig {
            username: None,
            password: None,
            auth_required: false,
            dns_resolve: true,
            allow_udp: false,
            request_timeout: 1,
        };

        // Try to resolve an invalid domain
        let target = TargetAddr::Domain("this-domain-does-not-exist-12345.invalid".to_string(), 80);
        let result = handle_tcp_connect(client, target, &config).await;
        assert!(result.is_err());
    }
}
