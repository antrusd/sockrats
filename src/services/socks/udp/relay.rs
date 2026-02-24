//! UDP relay for SOCKS5 data channels
//!
//! Bridges rathole `UdpTraffic` frames on the tunnel stream with actual
//! UDP destinations. Reads SOCKS5-encapsulated UDP packets from the tunnel,
//! forwards payload to the real destination, and sends responses back.

use super::{encode_udp_packet, parse_udp_packet, UdpPacket};
use crate::protocol::UdpTraffic;
use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::UdpSocket;
use tracing::{debug, warn};

/// Default UDP relay timeout in seconds
const UDP_RELAY_TIMEOUT_SECS: u64 = 120;

/// Maximum UDP packet size
const MAX_UDP_PACKET: usize = 65535;

/// Relay UDP traffic between a tunnel stream and real UDP destinations.
///
/// # Protocol
///
/// 1. Read `UdpTraffic` from the tunnel (rathole framing)
/// 2. Parse the inner SOCKS5 UDP header to extract destination + payload
/// 3. Send payload to the real destination via a bound UDP socket
/// 4. Receive response from the destination
/// 5. Wrap response in a SOCKS5 UDP header and write as `UdpTraffic`
pub struct UdpRelay {
    /// Timeout for individual UDP exchanges
    timeout_secs: u64,
}

impl UdpRelay {
    /// Create a new UDP relay with the default timeout.
    pub fn new() -> Self {
        UdpRelay {
            timeout_secs: UDP_RELAY_TIMEOUT_SECS,
        }
    }

    /// Set a custom timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Run the relay loop on the given tunnel stream.
    ///
    /// Reads `UdpTraffic` frames, forwards to UDP destinations, and writes
    /// responses back. Terminates on stream EOF or error.
    pub async fn run<S>(&self, mut stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        // Bind a single outbound UDP socket for all relay traffic
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Failed to bind UDP relay socket")?;

        let timeout = std::time::Duration::from_secs(self.timeout_secs);
        let mut recv_buf = vec![0u8; MAX_UDP_PACKET];

        loop {
            // Read the header length prefix from the tunnel
            let hdr_len = match stream.read_u8().await {
                Ok(len) => len,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    debug!("UDP tunnel stream closed");
                    break;
                }
                Err(e) => return Err(e.into()),
            };

            // Read the UdpTraffic frame
            let traffic = UdpTraffic::read(&mut stream, hdr_len)
                .await
                .context("Failed to read UdpTraffic")?;

            // Parse the SOCKS5 UDP encapsulation
            let socks_packet = match parse_udp_packet(&traffic.data) {
                Ok(pkt) => pkt,
                Err(e) => {
                    warn!("Invalid SOCKS5 UDP packet: {}", e);
                    continue;
                }
            };

            if socks_packet.is_fragmented() {
                warn!("Fragmented UDP packets not supported, dropping");
                continue;
            }

            // Resolve target address
            let target_addr = match socks_packet.addr.resolve().await {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Failed to resolve UDP target: {}", e);
                    continue;
                }
            };

            // Forward payload to target
            if let Err(e) = socket.send_to(&socks_packet.data, target_addr).await {
                warn!("UDP send to {} failed: {}", target_addr, e);
                continue;
            }

            debug!(
                "UDP relay: sent {} bytes to {}",
                socks_packet.data.len(),
                target_addr
            );

            // Wait for response with timeout
            match tokio::time::timeout(timeout, socket.recv_from(&mut recv_buf)).await {
                Ok(Ok((len, from_addr))) => {
                    debug!("UDP relay: received {} bytes from {}", len, from_addr);

                    // Wrap response in SOCKS5 UDP format
                    let response_packet =
                        UdpPacket::new(from_addr.into(), Bytes::copy_from_slice(&recv_buf[..len]));
                    let encoded = encode_udp_packet(&response_packet);

                    // Write back as UdpTraffic
                    let response_traffic = UdpTraffic::new(from_addr, Bytes::from(encoded));
                    response_traffic
                        .write(&mut stream)
                        .await
                        .context("Failed to write UDP response")?;
                }
                Ok(Err(e)) => {
                    warn!("UDP recv error: {}", e);
                }
                Err(_) => {
                    debug!("UDP response timeout for {}", target_addr);
                }
            }
        }

        Ok(())
    }
}

impl Default for UdpRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::socks::types::TargetAddr;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn test_udp_relay_new() {
        let relay = UdpRelay::new();
        assert_eq!(relay.timeout_secs, UDP_RELAY_TIMEOUT_SECS);
    }

    #[test]
    fn test_udp_relay_with_timeout() {
        let relay = UdpRelay::new().with_timeout(60);
        assert_eq!(relay.timeout_secs, 60);
    }

    #[test]
    fn test_udp_relay_default() {
        let relay = UdpRelay::default();
        assert_eq!(relay.timeout_secs, UDP_RELAY_TIMEOUT_SECS);
    }

    #[tokio::test]
    async fn test_udp_relay_eof_terminates() {
        let (writer, reader) = tokio::io::duplex(1024);

        // Close the writer immediately → relay should see EOF
        drop(writer);

        let relay = UdpRelay::new();
        // Use the reader side (which will see EOF)
        let result = relay.run(reader).await;
        // Should succeed (clean exit on EOF)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_udp_relay_invalid_packet_continues() {
        let (mut writer, reader) = tokio::io::duplex(4096);

        // Spawn the relay
        let relay_handle = tokio::spawn(async move {
            let relay = UdpRelay::new().with_timeout(1);
            relay.run(reader).await
        });

        // Write a valid UdpTraffic frame but with invalid SOCKS5 content
        let from: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let bad_socks_data = Bytes::from_static(&[0xFF, 0xFF]); // invalid SOCKS5 UDP
        let traffic = UdpTraffic::new(from, bad_socks_data);
        traffic.write(&mut writer).await.unwrap();

        // Close stream to terminate the relay
        drop(writer);

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), relay_handle).await;
        assert!(result.is_ok()); // relay terminated
    }

    #[tokio::test]
    async fn test_udp_relay_echo_integration() {
        // Start a local UDP echo server
        let echo_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo_socket.local_addr().unwrap();

        // Spawn echo server
        tokio::spawn(async move {
            let mut buf = [0u8; 65535];
            loop {
                match echo_socket.recv_from(&mut buf).await {
                    Ok((len, from)) => {
                        let _ = echo_socket.send_to(&buf[..len], from).await;
                    }
                    Err(_) => break,
                }
            }
        });

        let (mut writer, reader) = tokio::io::duplex(65536);

        // Spawn relay
        let _relay_handle = tokio::spawn(async move {
            let relay = UdpRelay::new().with_timeout(2);
            relay.run(reader).await
        });

        // Create a SOCKS5 UDP packet targeting the echo server
        let target = TargetAddr::ipv4(
            echo_addr.ip().to_string().parse::<Ipv4Addr>().unwrap(),
            echo_addr.port(),
        );
        let socks_pkt = UdpPacket::new(target, Bytes::from_static(b"hello echo"));
        let encoded = encode_udp_packet(&socks_pkt);

        // Write as UdpTraffic
        let traffic = UdpTraffic::new("127.0.0.1:5555".parse().unwrap(), Bytes::from(encoded));
        traffic.write(&mut writer).await.unwrap();

        // Read response UdpTraffic
        let (mut read_half, _write_half) = tokio::io::split(writer);
        let hdr_len = tokio::time::timeout(std::time::Duration::from_secs(2), read_half.read_u8())
            .await
            .unwrap()
            .unwrap();

        let response = UdpTraffic::read(&mut read_half, hdr_len).await.unwrap();

        // Parse the SOCKS5 response
        let resp_pkt = parse_udp_packet(&response.data).unwrap();
        assert_eq!(resp_pkt.data, Bytes::from_static(b"hello echo"));
    }
}
