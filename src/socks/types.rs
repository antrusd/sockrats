//! SOCKS5 type definitions
//!
//! Defines the core types used in SOCKS5 protocol handling.

use super::consts::*;
use anyhow::{Context, Result};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// SOCKS5 command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocksCommand {
    /// TCP CONNECT - establish a TCP connection to target
    Connect,
    /// TCP BIND - wait for incoming connection (not implemented)
    Bind,
    /// UDP ASSOCIATE - establish UDP relay
    UdpAssociate,
}

impl SocksCommand {
    /// Parse a command byte into SocksCommand
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            SOCKS5_CMD_TCP_CONNECT => Some(SocksCommand::Connect),
            SOCKS5_CMD_TCP_BIND => Some(SocksCommand::Bind),
            SOCKS5_CMD_UDP_ASSOCIATE => Some(SocksCommand::UdpAssociate),
            _ => None,
        }
    }

    /// Convert SocksCommand to byte
    pub fn to_byte(self) -> u8 {
        match self {
            SocksCommand::Connect => SOCKS5_CMD_TCP_CONNECT,
            SocksCommand::Bind => SOCKS5_CMD_TCP_BIND,
            SocksCommand::UdpAssociate => SOCKS5_CMD_UDP_ASSOCIATE,
        }
    }
}

impl fmt::Display for SocksCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocksCommand::Connect => write!(f, "CONNECT"),
            SocksCommand::Bind => write!(f, "BIND"),
            SocksCommand::UdpAssociate => write!(f, "UDP ASSOCIATE"),
        }
    }
}

/// Target address for SOCKS5 requests
///
/// Represents the destination address in a SOCKS5 request.
/// Can be an IP address (v4 or v6) or a domain name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetAddr {
    /// IP address with port
    Ip(SocketAddr),
    /// Domain name with port
    Domain(String, u16),
}

impl TargetAddr {
    /// Create a new TargetAddr from an IPv4 address and port
    pub fn ipv4(ip: Ipv4Addr, port: u16) -> Self {
        TargetAddr::Ip(SocketAddr::new(IpAddr::V4(ip), port))
    }

    /// Create a new TargetAddr from an IPv6 address and port
    pub fn ipv6(ip: Ipv6Addr, port: u16) -> Self {
        TargetAddr::Ip(SocketAddr::new(IpAddr::V6(ip), port))
    }

    /// Create a new TargetAddr from a domain name and port
    pub fn domain(domain: String, port: u16) -> Self {
        TargetAddr::Domain(domain, port)
    }

    /// Get the port number
    pub fn port(&self) -> u16 {
        match self {
            TargetAddr::Ip(addr) => addr.port(),
            TargetAddr::Domain(_, port) => *port,
        }
    }

    /// Get the address type byte for SOCKS5 protocol
    pub fn addr_type(&self) -> u8 {
        match self {
            TargetAddr::Ip(SocketAddr::V4(_)) => SOCKS5_ADDR_TYPE_IPV4,
            TargetAddr::Ip(SocketAddr::V6(_)) => SOCKS5_ADDR_TYPE_IPV6,
            TargetAddr::Domain(_, _) => SOCKS5_ADDR_TYPE_DOMAIN,
        }
    }

    /// Resolve the address to a SocketAddr
    ///
    /// For IP addresses, this returns immediately.
    /// For domain names, this performs DNS resolution.
    pub async fn resolve(&self) -> Result<SocketAddr> {
        match self {
            TargetAddr::Ip(addr) => Ok(*addr),
            TargetAddr::Domain(domain, port) => {
                let addr_str = format!("{}:{}", domain, port);
                let resolved = tokio::net::lookup_host(&addr_str)
                    .await
                    .with_context(|| format!("Failed to resolve domain: {}", domain))?
                    .next()
                    .with_context(|| format!("No addresses found for domain: {}", domain))?;
                Ok(resolved)
            }
        }
    }

    /// Serialize the address to bytes for SOCKS5 protocol
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            TargetAddr::Ip(SocketAddr::V4(addr)) => {
                bytes.push(SOCKS5_ADDR_TYPE_IPV4);
                bytes.extend_from_slice(&addr.ip().octets());
                bytes.extend_from_slice(&addr.port().to_be_bytes());
            }
            TargetAddr::Ip(SocketAddr::V6(addr)) => {
                bytes.push(SOCKS5_ADDR_TYPE_IPV6);
                bytes.extend_from_slice(&addr.ip().octets());
                bytes.extend_from_slice(&addr.port().to_be_bytes());
            }
            TargetAddr::Domain(domain, port) => {
                bytes.push(SOCKS5_ADDR_TYPE_DOMAIN);
                bytes.push(domain.len() as u8);
                bytes.extend_from_slice(domain.as_bytes());
                bytes.extend_from_slice(&port.to_be_bytes());
            }
        }

        bytes
    }
}

impl fmt::Display for TargetAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetAddr::Ip(addr) => write!(f, "{}", addr),
            TargetAddr::Domain(domain, port) => write!(f, "{}:{}", domain, port),
        }
    }
}

impl From<SocketAddr> for TargetAddr {
    fn from(addr: SocketAddr) -> Self {
        TargetAddr::Ip(addr)
    }
}

impl Default for TargetAddr {
    fn default() -> Self {
        TargetAddr::Ip(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks_command_from_byte() {
        assert_eq!(SocksCommand::from_byte(1), Some(SocksCommand::Connect));
        assert_eq!(SocksCommand::from_byte(2), Some(SocksCommand::Bind));
        assert_eq!(SocksCommand::from_byte(3), Some(SocksCommand::UdpAssociate));
        assert_eq!(SocksCommand::from_byte(4), None);
    }

    #[test]
    fn test_socks_command_to_byte() {
        assert_eq!(SocksCommand::Connect.to_byte(), 1);
        assert_eq!(SocksCommand::Bind.to_byte(), 2);
        assert_eq!(SocksCommand::UdpAssociate.to_byte(), 3);
    }

    #[test]
    fn test_socks_command_display() {
        assert_eq!(format!("{}", SocksCommand::Connect), "CONNECT");
        assert_eq!(format!("{}", SocksCommand::Bind), "BIND");
        assert_eq!(format!("{}", SocksCommand::UdpAssociate), "UDP ASSOCIATE");
    }

    #[test]
    fn test_target_addr_ipv4() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(192, 168, 1, 1), 8080);
        assert_eq!(addr.port(), 8080);
        assert_eq!(addr.addr_type(), SOCKS5_ADDR_TYPE_IPV4);
    }

    #[test]
    fn test_target_addr_ipv6() {
        let addr = TargetAddr::ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 443);
        assert_eq!(addr.port(), 443);
        assert_eq!(addr.addr_type(), SOCKS5_ADDR_TYPE_IPV6);
    }

    #[test]
    fn test_target_addr_domain() {
        let addr = TargetAddr::domain("example.com".to_string(), 80);
        assert_eq!(addr.port(), 80);
        assert_eq!(addr.addr_type(), SOCKS5_ADDR_TYPE_DOMAIN);
    }

    #[test]
    fn test_target_addr_display() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(127, 0, 0, 1), 8080);
        assert_eq!(format!("{}", addr), "127.0.0.1:8080");

        let addr = TargetAddr::domain("test.com".to_string(), 443);
        assert_eq!(format!("{}", addr), "test.com:443");
    }

    #[test]
    fn test_target_addr_to_bytes_ipv4() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(192, 168, 1, 1), 8080);
        let bytes = addr.to_bytes();

        assert_eq!(bytes[0], SOCKS5_ADDR_TYPE_IPV4);
        assert_eq!(&bytes[1..5], &[192, 168, 1, 1]);
        assert_eq!(&bytes[5..7], &8080u16.to_be_bytes());
    }

    #[test]
    fn test_target_addr_to_bytes_domain() {
        let addr = TargetAddr::domain("test".to_string(), 80);
        let bytes = addr.to_bytes();

        assert_eq!(bytes[0], SOCKS5_ADDR_TYPE_DOMAIN);
        assert_eq!(bytes[1], 4); // "test" length
        assert_eq!(&bytes[2..6], b"test");
        assert_eq!(&bytes[6..8], &80u16.to_be_bytes());
    }

    #[tokio::test]
    async fn test_target_addr_resolve_ip() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let resolved = addr.resolve().await.unwrap();
        assert_eq!(resolved.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(resolved.port(), 8080);
    }

    #[test]
    fn test_target_addr_from_socket_addr() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1234);
        let target: TargetAddr = socket_addr.into();
        assert_eq!(target, TargetAddr::Ip(socket_addr));
    }
}
