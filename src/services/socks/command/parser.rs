//! SOCKS5 command parser
//!
//! Parses SOCKS5 command requests from the client.

use crate::services::socks::consts::*;
use crate::services::socks::types::{SocksCommand, TargetAddr};
use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Parse a SOCKS5 command from the stream
///
/// # SOCKS5 Request Format
///
/// ```text
/// +----+-----+-------+------+----------+----------+
/// |VER | CMD |  RSV  | ATYP | DST.ADDR | DST.PORT |
/// +----+-----+-------+------+----------+----------+
/// | 1  |  1  | X'00' |  1   | Variable |    2     |
/// +----+-----+-------+------+----------+----------+
/// ```
///
/// # Arguments
///
/// * `stream` - The stream to read from
/// * `resolve_dns` - Whether to resolve domain names immediately
///
/// # Returns
///
/// A tuple of (command, target_address)
pub async fn parse_command<S>(
    stream: &mut S,
    resolve_dns: bool,
) -> Result<(SocksCommand, TargetAddr)>
where
    S: AsyncRead + Unpin,
{
    // Read: VER CMD RSV ATYP
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .with_context(|| "Failed to read command header")?;

    let version = header[0];
    let cmd_byte = header[1];
    let _reserved = header[2];
    let addr_type = header[3];

    // Validate version
    if version != SOCKS5_VERSION {
        bail!("Unsupported SOCKS version in command: {}", version);
    }

    // Parse command
    let command = SocksCommand::from_byte(cmd_byte)
        .ok_or_else(|| anyhow::anyhow!("Unknown command: {}", cmd_byte))?;

    // Parse target address based on type
    let target_addr = parse_address(stream, addr_type, resolve_dns).await?;

    tracing::debug!("Parsed SOCKS5 command: {} to {}", command, target_addr);

    Ok((command, target_addr))
}

/// Parse the address portion of a SOCKS5 request
async fn parse_address<S>(stream: &mut S, addr_type: u8, resolve_dns: bool) -> Result<TargetAddr>
where
    S: AsyncRead + Unpin,
{
    match addr_type {
        SOCKS5_ADDR_TYPE_IPV4 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);

            Ok(TargetAddr::ipv4(Ipv4Addr::from(addr), port))
        }

        SOCKS5_ADDR_TYPE_DOMAIN => {
            // Read domain length
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let domain_len = len_buf[0] as usize;

            if domain_len == 0 || domain_len > MAX_DOMAIN_LEN {
                bail!("Invalid domain length: {}", domain_len);
            }

            // Read domain name
            let mut domain_buf = vec![0u8; domain_len];
            stream.read_exact(&mut domain_buf).await?;
            let domain =
                String::from_utf8(domain_buf).with_context(|| "Invalid UTF-8 in domain name")?;

            // Read port
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);

            let target = TargetAddr::domain(domain, port);

            // Optionally resolve DNS immediately
            if resolve_dns {
                let resolved = target.resolve().await?;
                Ok(TargetAddr::Ip(resolved))
            } else {
                Ok(target)
            }
        }

        SOCKS5_ADDR_TYPE_IPV6 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);

            Ok(TargetAddr::ipv6(Ipv6Addr::from(addr), port))
        }

        _ => bail!("Unsupported address type: {}", addr_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_connect_request_ipv4(ip: [u8; 4], port: u16) -> Vec<u8> {
        let mut request = vec![
            SOCKS5_VERSION,
            SOCKS5_CMD_TCP_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ADDR_TYPE_IPV4,
        ];
        request.extend_from_slice(&ip);
        request.extend_from_slice(&port.to_be_bytes());
        request
    }

    fn create_connect_request_domain(domain: &str, port: u16) -> Vec<u8> {
        let mut request = vec![
            SOCKS5_VERSION,
            SOCKS5_CMD_TCP_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ADDR_TYPE_DOMAIN,
            domain.len() as u8,
        ];
        request.extend_from_slice(domain.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        request
    }

    fn create_connect_request_ipv6(ip: [u8; 16], port: u16) -> Vec<u8> {
        let mut request = vec![
            SOCKS5_VERSION,
            SOCKS5_CMD_TCP_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ADDR_TYPE_IPV6,
        ];
        request.extend_from_slice(&ip);
        request.extend_from_slice(&port.to_be_bytes());
        request
    }

    #[tokio::test]
    async fn test_parse_command_ipv4() {
        let request = create_connect_request_ipv4([192, 168, 1, 1], 8080);
        let mut cursor = Cursor::new(request);

        let (cmd, addr) = parse_command(&mut cursor, false).await.unwrap();

        assert_eq!(cmd, SocksCommand::Connect);
        match addr {
            TargetAddr::Ip(socket_addr) => {
                assert_eq!(socket_addr.ip().to_string(), "192.168.1.1");
                assert_eq!(socket_addr.port(), 8080);
            }
            _ => panic!("Expected IPv4 address"),
        }
    }

    #[tokio::test]
    async fn test_parse_command_domain_no_resolve() {
        let request = create_connect_request_domain("example.com", 443);
        let mut cursor = Cursor::new(request);

        let (cmd, addr) = parse_command(&mut cursor, false).await.unwrap();

        assert_eq!(cmd, SocksCommand::Connect);
        match addr {
            TargetAddr::Domain(domain, port) => {
                assert_eq!(domain, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected domain address"),
        }
    }

    #[tokio::test]
    async fn test_parse_command_ipv6() {
        let ip = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let request = create_connect_request_ipv6(ip, 80);
        let mut cursor = Cursor::new(request);

        let (cmd, addr) = parse_command(&mut cursor, false).await.unwrap();

        assert_eq!(cmd, SocksCommand::Connect);
        match addr {
            TargetAddr::Ip(socket_addr) => {
                assert!(socket_addr.ip().is_ipv6());
                assert_eq!(socket_addr.port(), 80);
            }
            _ => panic!("Expected IPv6 address"),
        }
    }

    #[tokio::test]
    async fn test_parse_command_invalid_version() {
        let mut request = create_connect_request_ipv4([127, 0, 0, 1], 80);
        request[0] = 4; // SOCKS4

        let mut cursor = Cursor::new(request);
        let result = parse_command(&mut cursor, false).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[tokio::test]
    async fn test_parse_command_unknown_command() {
        let mut request = create_connect_request_ipv4([127, 0, 0, 1], 80);
        request[1] = 0x99; // Unknown command

        let mut cursor = Cursor::new(request);
        let result = parse_command(&mut cursor, false).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_command_udp_associate() {
        let mut request = create_connect_request_ipv4([0, 0, 0, 0], 0);
        request[1] = SOCKS5_CMD_UDP_ASSOCIATE;

        let mut cursor = Cursor::new(request);
        let (cmd, _) = parse_command(&mut cursor, false).await.unwrap();

        assert_eq!(cmd, SocksCommand::UdpAssociate);
    }
}
