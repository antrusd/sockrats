//! Boringtun `Tunn` wrapper with pre-allocated buffers.
//!
//! [`TunnelHandle`] wraps `boringtun::noise::Tunn` and provides a
//! simplified interface for the event loop.  Pre-allocated buffers
//! avoid heap allocation in the packet-processing hot path.

use anyhow::Result;
use boringtun::noise::{Tunn, TunnResult};
use tracing::{debug, trace, warn};
use x25519_dalek::{PublicKey, StaticSecret};

use super::config::WireguardConfig;

/// Overhead added by WireGuard encapsulation (header + auth tag).
const WG_OVERHEAD: usize = 80;

/// Minimum size for the encapsulation destination buffer.
/// Must be at least 148 bytes (handshake init size).
const MIN_ENCAP_BUF: usize = 148;

/// Size of pre-allocated packet buffers.
const BUF_SIZE: usize = 1500 + WG_OVERHEAD;

/// Result of an encapsulate operation.
pub enum EncapResult<'a> {
    /// Encrypted WireGuard packet ready to send over UDP.
    Packet(&'a [u8]),
    /// No active session — a handshake-init packet was produced instead.
    /// The original data has been queued internally by boringtun.
    HandshakeInit(&'a [u8]),
    /// Nothing to send (e.g. packet queued, handshake in progress).
    Done,
}

/// Result of a decapsulate operation.
pub enum DecapResult<'a> {
    /// Decrypted IP packet — inject into the smoltcp virtual device.
    IpPacket(&'a [u8]),
    /// A WireGuard control packet to send back over UDP
    /// (handshake response, cookie reply, keepalive, etc.).
    SendToNetwork(&'a [u8]),
    /// Nothing to do.
    Done,
}

/// Wraps a boringtun [`Tunn`] with pre-allocated buffers.
pub struct TunnelHandle {
    /// The boringtun tunnel instance (not `Send`, must stay on one thread).
    tunn: Box<Tunn>,
    /// Shared buffer for `encapsulate()` output.
    enc_buf: Vec<u8>,
    /// Shared buffer for `decapsulate()` output.
    dec_buf: Vec<u8>,
    /// Shared buffer for `update_timers()` output.
    timer_buf: Vec<u8>,
}

impl TunnelHandle {
    /// Create a new tunnel from a [`WireguardConfig`].
    pub fn new(config: &WireguardConfig) -> Result<Self> {
        let private_key_bytes = config.decode_private_key()?;
        let peer_pub_bytes = config.decode_peer_public_key()?;
        let preshared_key = config.decode_preshared_key()?;
        let keepalive = config.keepalive_interval();

        // Build x25519 key objects
        let static_private = StaticSecret::from(private_key_bytes);
        let peer_public = PublicKey::from(peer_pub_bytes);

        // Index 0 is fine for a single-peer tunnel.
        let tunn = Tunn::new(
            static_private,
            peer_public,
            preshared_key,
            keepalive,
            0,
            None, // No rate limiter for a client tunnel
        );

        debug!("WireGuard tunnel created (keepalive={:?})", keepalive);

        Ok(Self {
            tunn: Box::new(tunn),
            enc_buf: vec![0u8; BUF_SIZE],
            dec_buf: vec![0u8; BUF_SIZE],
            timer_buf: vec![0u8; BUF_SIZE],
        })
    }

    /// Encapsulate a plaintext IP packet for transmission over the tunnel.
    ///
    /// Returns an [`EncapResult`] indicating the action to take.
    /// The returned slice borrows from the internal `enc_buf`.
    pub fn encapsulate(&mut self, src: &[u8]) -> EncapResult<'_> {
        // Ensure buffer is large enough
        let needed = std::cmp::max(src.len() + WG_OVERHEAD, MIN_ENCAP_BUF);
        if self.enc_buf.len() < needed {
            self.enc_buf.resize(needed, 0);
        }

        match self.tunn.encapsulate(src, &mut self.enc_buf) {
            TunnResult::WriteToNetwork(data) => {
                // Check if this is a handshake init (no active session yet)
                // by examining the first byte: type 1 = handshake init
                if !src.is_empty() && data.len() >= 4 && data[0] == 1 {
                    trace!("encapsulate: handshake init ({} bytes)", data.len());
                    EncapResult::HandshakeInit(data)
                } else {
                    trace!("encapsulate: data packet ({} bytes)", data.len());
                    EncapResult::Packet(data)
                }
            }
            TunnResult::Done => {
                trace!("encapsulate: done (packet queued or no action)");
                EncapResult::Done
            }
            TunnResult::Err(e) => {
                warn!("encapsulate error: {:?}", e);
                EncapResult::Done
            }
            _ => {
                // WriteToTunnelV4/V6 shouldn't happen for encapsulate
                warn!("encapsulate: unexpected TunnResult variant");
                EncapResult::Done
            }
        }
    }

    /// Decapsulate a received encrypted UDP datagram.
    ///
    /// Returns a [`DecapResult`] indicating the action to take.
    /// The returned slice borrows from the internal `dec_buf`.
    pub fn decapsulate(&mut self, src: &[u8]) -> DecapResult<'_> {
        match self.tunn.decapsulate(None, src, &mut self.dec_buf) {
            TunnResult::WriteToTunnelV4(data, _addr) => {
                trace!("decapsulate: IPv4 packet ({} bytes)", data.len());
                DecapResult::IpPacket(data)
            }
            TunnResult::WriteToTunnelV6(data, _addr) => {
                trace!("decapsulate: IPv6 packet ({} bytes)", data.len());
                DecapResult::IpPacket(data)
            }
            TunnResult::WriteToNetwork(data) => {
                trace!(
                    "decapsulate: send-to-network ({} bytes, type={})",
                    data.len(),
                    if data.is_empty() { 0 } else { data[0] }
                );
                DecapResult::SendToNetwork(data)
            }
            TunnResult::Done => {
                trace!("decapsulate: done");
                DecapResult::Done
            }
            TunnResult::Err(e) => {
                warn!("decapsulate error: {:?}", e);
                DecapResult::Done
            }
        }
    }

    /// Process a decapsulate result that produced a "send to network" packet,
    /// and continue decapsulating the empty packet to flush any queued data.
    ///
    /// boringtun may produce multiple packets in sequence (e.g. handshake
    /// response followed by queued data).  Call this in a loop after
    /// [`decapsulate()`] returns `SendToNetwork`.
    pub fn decapsulate_flush(&mut self) -> DecapResult<'_> {
        match self.tunn.decapsulate(None, &[], &mut self.dec_buf) {
            TunnResult::WriteToTunnelV4(data, _addr) => {
                trace!("decapsulate_flush: IPv4 packet ({} bytes)", data.len());
                DecapResult::IpPacket(data)
            }
            TunnResult::WriteToTunnelV6(data, _addr) => {
                trace!("decapsulate_flush: IPv6 packet ({} bytes)", data.len());
                DecapResult::IpPacket(data)
            }
            TunnResult::WriteToNetwork(data) => {
                trace!("decapsulate_flush: send-to-network ({} bytes)", data.len());
                DecapResult::SendToNetwork(data)
            }
            TunnResult::Done => DecapResult::Done,
            TunnResult::Err(e) => {
                warn!("decapsulate_flush error: {:?}", e);
                DecapResult::Done
            }
        }
    }

    /// Tick the internal timers (keepalive, rekey, handshake retry).
    ///
    /// Returns packets to send, if any.  The caller should invoke this
    /// every ~250 ms.  Results must be sent in order.
    pub fn update_timers(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        loop {
            match self.tunn.update_timers(&mut self.timer_buf) {
                TunnResult::WriteToNetwork(data) => {
                    trace!("timer: send packet ({} bytes)", data.len());
                    packets.push(data.to_vec());
                }
                TunnResult::Done => break,
                TunnResult::Err(e) => {
                    warn!("timer error: {:?}", e);
                    break;
                }
                _ => break,
            }
        }
        packets
    }

    /// Force a handshake initiation.
    ///
    /// Useful for establishing the tunnel at startup.
    pub fn force_handshake(&mut self) -> Option<Vec<u8>> {
        match self
            .tunn
            .format_handshake_initiation(&mut self.enc_buf, false)
        {
            TunnResult::WriteToNetwork(data) => {
                debug!("Forced handshake initiation ({} bytes)", data.len());
                Some(data.to_vec())
            }
            TunnResult::Done => None,
            TunnResult::Err(e) => {
                warn!("force_handshake error: {:?}", e);
                None
            }
            _ => None,
        }
    }
}

impl std::fmt::Debug for TunnelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunnelHandle")
            .field("enc_buf_size", &self.enc_buf.len())
            .field("dec_buf_size", &self.dec_buf.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    /// Generate a valid WireGuard keypair for testing.
    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        use rand::rngs::OsRng;
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        (secret.to_bytes(), public.to_bytes())
    }

    fn make_test_config() -> WireguardConfig {
        let (client_priv, _client_pub) = test_keypair();
        let (_server_priv, server_pub) = test_keypair();

        WireguardConfig {
            enabled: true,
            private_key: BASE64.encode(client_priv),
            peer_public_key: BASE64.encode(server_pub),
            preshared_key: None,
            peer_endpoint: "127.0.0.1:51820".to_string(),
            persistent_keepalive: 25,
            address: "10.0.0.2/24".to_string(),
            allowed_ips: vec!["10.0.0.0/24".to_string()],
        }
    }

    #[test]
    fn test_create_tunnel() {
        let cfg = make_test_config();
        let tunnel = TunnelHandle::new(&cfg);
        assert!(tunnel.is_ok());
    }

    #[test]
    fn test_create_tunnel_with_psk() {
        let mut cfg = make_test_config();
        cfg.preshared_key = Some(BASE64.encode([0xAB; 32]));
        let tunnel = TunnelHandle::new(&cfg);
        assert!(tunnel.is_ok());
    }

    #[test]
    fn test_encapsulate_no_session() {
        let cfg = make_test_config();
        let mut tunnel = TunnelHandle::new(&cfg).unwrap();

        // Minimal IPv4 header (20 bytes)
        let ip_packet = [
            0x45, 0x00, 0x00, 0x14, // version, IHL, total length
            0x00, 0x00, 0x00, 0x00, // identification, flags, fragment
            0x40, 0x06, 0x00, 0x00, // TTL, TCP protocol, checksum
            0x0a, 0x00, 0x00, 0x02, // src: 10.0.0.2
            0x0a, 0x00, 0x00, 0x01, // dst: 10.0.0.1
        ];

        // First encapsulate without a session should trigger handshake
        let result = tunnel.encapsulate(&ip_packet);
        match result {
            EncapResult::HandshakeInit(pkt) => {
                assert!(!pkt.is_empty());
                assert_eq!(pkt[0], 1); // Type 1 = handshake initiation
            }
            EncapResult::Done => {
                // Also acceptable — data was queued
            }
            _ => panic!("Expected HandshakeInit or Done"),
        }
    }

    #[test]
    fn test_decapsulate_garbage() {
        let cfg = make_test_config();
        let mut tunnel = TunnelHandle::new(&cfg).unwrap();

        // Random garbage should result in Done or error (not panic)
        let garbage = [0xFF; 100];
        let result = tunnel.decapsulate(&garbage);
        match result {
            DecapResult::Done => {}
            DecapResult::SendToNetwork(_) => {}
            DecapResult::IpPacket(_) => panic!("Should not produce IP from garbage"),
        }
    }

    #[test]
    fn test_update_timers_initial() {
        let cfg = make_test_config();
        let mut tunnel = TunnelHandle::new(&cfg).unwrap();

        // Initial timer update may or may not produce packets
        let packets = tunnel.update_timers();
        // Just verify it doesn't panic
        assert!(packets.len() <= 10);
    }

    #[test]
    fn test_force_handshake() {
        let cfg = make_test_config();
        let mut tunnel = TunnelHandle::new(&cfg).unwrap();

        let pkt = tunnel.force_handshake();
        assert!(pkt.is_some());
        let pkt = pkt.unwrap();
        assert!(!pkt.is_empty());
        assert_eq!(pkt[0], 1); // Type 1 = handshake initiation
    }

    #[test]
    fn test_debug_impl() {
        let cfg = make_test_config();
        let tunnel = TunnelHandle::new(&cfg).unwrap();
        let debug = format!("{:?}", tunnel);
        assert!(debug.contains("TunnelHandle"));
    }
}
