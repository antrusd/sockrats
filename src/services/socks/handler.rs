//! Main SOCKS5 handler
//!
//! This module provides the main entry point for handling SOCKS5 requests
//! on tunnel streams. It orchestrates authentication, command parsing,
//! and request handling.

use crate::config::SocksConfig;
use crate::services::socks::auth::authenticate;
use crate::services::socks::command::{parse_command, send_command_not_supported};
use crate::services::socks::tcp_relay::handle_tcp_connect;
use crate::services::socks::types::SocksCommand;
use crate::services::socks::udp::handle_udp_associate;
use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, info, warn};

/// Handle SOCKS5 protocol on a stream
///
/// This is the main entry point for processing SOCKS5 requests.
/// The stream comes directly from the rathole tunnel, so there's
/// no local socket binding involved.
///
/// # Protocol Flow
///
/// 1. Authentication negotiation
/// 2. Username/password authentication (if required)
/// 3. Command parsing
/// 4. Command execution (CONNECT, BIND, or UDP ASSOCIATE)
///
/// # Arguments
///
/// * `stream` - The tunnel stream to process
/// * `config` - SOCKS5 configuration
///
/// # Returns
///
/// Ok(()) if the request was handled successfully, Err otherwise
pub async fn handle_socks5_on_stream<S>(mut stream: S, config: &SocksConfig) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // Step 1: Authentication negotiation
    let auth_method = authenticate(&mut stream, config)
        .await
        .with_context(|| "Authentication negotiation failed")?;

    debug!("Authentication completed with method: {:?}", auth_method);

    // Step 2: Read and parse the SOCKS5 command
    let (command, target_addr) = parse_command(&mut stream, config.dns_resolve)
        .await
        .with_context(|| "Failed to parse SOCKS5 command")?;

    info!("SOCKS5 {} request to {}", command, target_addr);

    // Step 3: Execute the command
    match command {
        SocksCommand::Connect => {
            handle_tcp_connect(stream, target_addr, config).await?;
        }
        SocksCommand::UdpAssociate => {
            if config.allow_udp {
                handle_udp_associate(stream, target_addr, config).await?;
            } else {
                warn!("UDP ASSOCIATE not allowed by configuration");
                send_command_not_supported(&mut stream).await?;
            }
        }
        SocksCommand::Bind => {
            // BIND is not supported in reverse tunnel mode
            warn!("BIND command not supported");
            send_command_not_supported(&mut stream).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::socks::consts::*;

    // Helper to create a mock SOCKS5 handshake
    fn create_socks5_handshake(auth_method: u8, command: u8, addr: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();

        // Auth negotiation: version, num_methods, methods
        data.push(SOCKS5_VERSION);
        data.push(1); // 1 method
        data.push(auth_method);

        // Command: version, cmd, rsv, atyp, addr, port
        data.push(SOCKS5_VERSION);
        data.push(command);
        data.push(SOCKS5_RESERVED);
        data.extend_from_slice(addr);

        data
    }

    #[test]
    fn test_create_handshake_no_auth() {
        let addr = &[
            SOCKS5_ADDR_TYPE_IPV4,
            127,
            0,
            0,
            1, // IP
            0x1F,
            0x90, // Port 8080
        ];

        let handshake =
            create_socks5_handshake(SOCKS5_AUTH_METHOD_NONE, SOCKS5_CMD_TCP_CONNECT, addr);

        // Verify auth negotiation
        assert_eq!(handshake[0], SOCKS5_VERSION);
        assert_eq!(handshake[1], 1);
        assert_eq!(handshake[2], SOCKS5_AUTH_METHOD_NONE);

        // Verify command
        assert_eq!(handshake[3], SOCKS5_VERSION);
        assert_eq!(handshake[4], SOCKS5_CMD_TCP_CONNECT);
    }

    #[tokio::test]
    async fn test_handle_socks5_requires_valid_version() {
        // Test that invalid SOCKS version is rejected
        let _config = SocksConfig::default();

        // Invalid SOCKS version
        let _data = vec![
            0x04, // SOCKS4 version (invalid)
            1,
            SOCKS5_AUTH_METHOD_NONE,
        ];

        // This would fail early in authentication
        // We can't fully test without a bidirectional mock stream
    }
}
