//! UDP packet encoding/decoding for SOCKS5
//!
//! Handles the encapsulation format for UDP packets in SOCKS5.

use crate::services::socks::consts::*;
use crate::services::socks::types::TargetAddr;
use anyhow::{bail, Context, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::net::{Ipv4Addr, Ipv6Addr};

/// UDP packet structure for SOCKS5
///
/// # UDP Request/Response Format
///
/// ```text
/// +----+------+------+----------+----------+----------+
/// |RSV | FRAG | ATYP | DST.ADDR | DST.PORT |   DATA   |
/// +----+------+------+----------+----------+----------+
/// | 2  |  1   |  1   | Variable |    2     | Variable |
/// +----+------+------+----------+----------+----------+
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UdpPacket {
    /// Fragment number (0 for standalone packets)
    pub frag: u8,
    /// Target/source address
    pub addr: TargetAddr,
    /// Packet data
    pub data: Bytes,
}

#[allow(dead_code)]
impl UdpPacket {
    /// Create a new UDP packet
    pub fn new(addr: TargetAddr, data: Bytes) -> Self {
        UdpPacket {
            frag: 0,
            addr,
            data,
        }
    }

    /// Create a new fragmented UDP packet
    pub fn with_frag(frag: u8, addr: TargetAddr, data: Bytes) -> Self {
        UdpPacket { frag, addr, data }
    }

    /// Check if this is a fragmented packet
    pub fn is_fragmented(&self) -> bool {
        self.frag != 0
    }
}

/// Parse a UDP packet from bytes
///
/// # Arguments
///
/// * `data` - The raw packet data
///
/// # Returns
///
/// Parsed UdpPacket if successful
#[allow(dead_code)]
pub fn parse_udp_packet(data: &[u8]) -> Result<UdpPacket> {
    if data.len() < 4 {
        bail!("UDP packet too short: {} bytes", data.len());
    }

    let mut buf = data;

    // RSV (2 bytes) - must be 0
    let rsv = buf.get_u16();
    if rsv != 0 {
        bail!("Invalid RSV field: {}", rsv);
    }

    // FRAG (1 byte)
    let frag = buf.get_u8();

    // ATYP (1 byte)
    let atyp = buf.get_u8();

    // Parse address based on type
    let (addr, remaining) = parse_address_from_buf(atyp, buf)?;

    // Remaining data is the payload
    let data = Bytes::copy_from_slice(remaining);

    Ok(UdpPacket { frag, addr, data })
}

/// Parse address from buffer
fn parse_address_from_buf(atyp: u8, mut buf: &[u8]) -> Result<(TargetAddr, &[u8])> {
    match atyp {
        SOCKS5_ADDR_TYPE_IPV4 => {
            if buf.len() < 6 {
                bail!("Buffer too short for IPv4 address");
            }
            let ip = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
            buf = &buf[4..];
            let port = buf.get_u16();
            Ok((TargetAddr::ipv4(ip, port), buf))
        }

        SOCKS5_ADDR_TYPE_DOMAIN => {
            if buf.is_empty() {
                bail!("Buffer too short for domain length");
            }
            let len = buf[0] as usize;
            buf = &buf[1..];

            if buf.len() < len + 2 {
                bail!("Buffer too short for domain name");
            }
            let domain = String::from_utf8(buf[..len].to_vec())
                .with_context(|| "Invalid UTF-8 in domain")?;
            buf = &buf[len..];
            let port = buf.get_u16();
            Ok((TargetAddr::domain(domain, port), buf))
        }

        SOCKS5_ADDR_TYPE_IPV6 => {
            if buf.len() < 18 {
                bail!("Buffer too short for IPv6 address");
            }
            let mut ip_bytes = [0u8; 16];
            ip_bytes.copy_from_slice(&buf[..16]);
            let ip = Ipv6Addr::from(ip_bytes);
            buf = &buf[16..];
            let port = buf.get_u16();
            Ok((TargetAddr::ipv6(ip, port), buf))
        }

        _ => bail!("Unknown address type: {}", atyp),
    }
}

/// Encode a UDP packet to bytes
///
/// # Arguments
///
/// * `packet` - The packet to encode
///
/// # Returns
///
/// Encoded bytes
#[allow(dead_code)]
pub fn encode_udp_packet(packet: &UdpPacket) -> Vec<u8> {
    let mut buf = BytesMut::new();

    // RSV (2 bytes)
    buf.put_u16(0);

    // FRAG (1 byte)
    buf.put_u8(packet.frag);

    // Address
    buf.extend_from_slice(&packet.addr.to_bytes());

    // Data
    buf.extend_from_slice(&packet.data);

    buf.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udp_packet_new() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(192, 168, 1, 1), 8080);
        let data = Bytes::from_static(b"hello");
        let packet = UdpPacket::new(addr, data);

        assert_eq!(packet.frag, 0);
        assert!(!packet.is_fragmented());
    }

    #[test]
    fn test_udp_packet_with_frag() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(127, 0, 0, 1), 1234);
        let data = Bytes::from_static(b"data");
        let packet = UdpPacket::with_frag(1, addr, data);

        assert_eq!(packet.frag, 1);
        assert!(packet.is_fragmented());
    }

    #[test]
    fn test_encode_udp_packet_ipv4() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(10, 0, 0, 1), 80);
        let data = Bytes::from_static(b"test");
        let packet = UdpPacket::new(addr, data);

        let encoded = encode_udp_packet(&packet);

        // RSV (2) + FRAG (1) + ATYP (1) + IPv4 (4) + PORT (2) + DATA (4)
        assert_eq!(encoded.len(), 2 + 1 + 1 + 4 + 2 + 4);

        // Check RSV
        assert_eq!(&encoded[0..2], &[0, 0]);
        // Check FRAG
        assert_eq!(encoded[2], 0);
        // Check ATYP
        assert_eq!(encoded[3], SOCKS5_ADDR_TYPE_IPV4);
        // Check IP
        assert_eq!(&encoded[4..8], &[10, 0, 0, 1]);
        // Check port (big endian)
        assert_eq!(&encoded[8..10], &80u16.to_be_bytes());
        // Check data
        assert_eq!(&encoded[10..], b"test");
    }

    #[test]
    fn test_encode_udp_packet_domain() {
        let addr = TargetAddr::domain("test.com".to_string(), 443);
        let data = Bytes::from_static(b"hi");
        let packet = UdpPacket::new(addr, data);

        let encoded = encode_udp_packet(&packet);

        // RSV (2) + FRAG (1) + ATYP (1) + LEN (1) + DOMAIN (8) + PORT (2) + DATA (2)
        assert_eq!(encoded.len(), 2 + 1 + 1 + 1 + 8 + 2 + 2);

        assert_eq!(encoded[3], SOCKS5_ADDR_TYPE_DOMAIN);
        assert_eq!(encoded[4], 8); // "test.com" length
        assert_eq!(&encoded[5..13], b"test.com");
    }

    #[test]
    fn test_parse_udp_packet_ipv4() {
        let addr = TargetAddr::ipv4(Ipv4Addr::new(192, 168, 1, 100), 9999);
        let data = Bytes::from_static(b"payload");
        let original = UdpPacket::new(addr, data);

        let encoded = encode_udp_packet(&original);
        let parsed = parse_udp_packet(&encoded).unwrap();

        assert_eq!(parsed.frag, 0);
        assert_eq!(parsed.data, Bytes::from_static(b"payload"));

        match parsed.addr {
            TargetAddr::Ip(socket_addr) => {
                assert_eq!(socket_addr.ip().to_string(), "192.168.1.100");
                assert_eq!(socket_addr.port(), 9999);
            }
            _ => panic!("Expected IPv4 address"),
        }
    }

    #[test]
    fn test_parse_udp_packet_domain() {
        let addr = TargetAddr::domain("example.org".to_string(), 8080);
        let data = Bytes::from_static(b"content");
        let original = UdpPacket::new(addr, data);

        let encoded = encode_udp_packet(&original);
        let parsed = parse_udp_packet(&encoded).unwrap();

        match parsed.addr {
            TargetAddr::Domain(domain, port) => {
                assert_eq!(domain, "example.org");
                assert_eq!(port, 8080);
            }
            _ => panic!("Expected domain address"),
        }
    }

    #[test]
    fn test_parse_udp_packet_too_short() {
        let result = parse_udp_packet(&[0, 0, 0]); // Only 3 bytes
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_udp_packet_invalid_rsv() {
        let mut data = encode_udp_packet(&UdpPacket::new(
            TargetAddr::ipv4(Ipv4Addr::new(0, 0, 0, 0), 0),
            Bytes::new(),
        ));
        data[0] = 1; // Invalid RSV

        let result = parse_udp_packet(&data);
        assert!(result.is_err());
    }
}
