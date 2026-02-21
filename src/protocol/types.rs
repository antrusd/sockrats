//! Protocol type definitions
//!
//! These types must be compatible with rathole's protocol.rs

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Hash width in bytes (SHA-256 produces 32 bytes)
pub const HASH_WIDTH_IN_BYTES: usize = 32;

/// Protocol version type
type ProtocolVersion = u8;

/// Protocol version 0 (deprecated)
const _PROTO_V0: u8 = 0u8;

/// Protocol version 1 (current)
const PROTO_V1: u8 = 1u8;

/// Current protocol version
pub const CURRENT_PROTO_VERSION: ProtocolVersion = PROTO_V1;

/// Digest type (32-byte SHA-256 hash)
pub type Digest = [u8; HASH_WIDTH_IN_BYTES];

/// Hello message for establishing channels
///
/// This is the first message sent when establishing a control or data channel.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum Hello {
    /// Control channel hello: (protocol version, sha256(service name) or nonce)
    ControlChannelHello(ProtocolVersion, Digest),
    /// Data channel hello: (protocol version, session key)
    DataChannelHello(ProtocolVersion, Digest),
}

impl Hello {
    /// Create a new control channel hello message
    pub fn control_channel(service_name: &str) -> Self {
        let service_digest = super::digest::digest(service_name.as_bytes());
        Hello::ControlChannelHello(CURRENT_PROTO_VERSION, service_digest)
    }

    /// Create a new data channel hello message
    pub fn data_channel(session_key: Digest) -> Self {
        Hello::DataChannelHello(CURRENT_PROTO_VERSION, session_key)
    }

    /// Get the protocol version from the hello message
    pub fn version(&self) -> ProtocolVersion {
        match self {
            Hello::ControlChannelHello(v, _) => *v,
            Hello::DataChannelHello(v, _) => *v,
        }
    }

    /// Get the digest from the hello message
    pub fn digest(&self) -> &Digest {
        match self {
            Hello::ControlChannelHello(_, d) => d,
            Hello::DataChannelHello(_, d) => d,
        }
    }
}

/// Authentication message
///
/// Sent by the client after receiving the server's nonce.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Auth(pub Digest);

impl Auth {
    /// Create a new auth message from token and nonce
    pub fn new(token: &str, nonce: &Digest) -> Self {
        let mut concat = Vec::from(token.as_bytes());
        concat.extend_from_slice(nonce);
        Auth(super::digest::digest(&concat))
    }
}

/// Acknowledgment message
///
/// Sent by the server in response to authentication.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum Ack {
    /// Authentication successful
    Ok,
    /// Service does not exist on server
    ServiceNotExist,
    /// Authentication failed (wrong token)
    AuthFailed,
}

impl std::fmt::Display for Ack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Ack::Ok => "Ok",
                Ack::ServiceNotExist => "Service not exist",
                Ack::AuthFailed => "Incorrect token",
            }
        )
    }
}

impl Ack {
    /// Check if the acknowledgment indicates success
    pub fn is_ok(&self) -> bool {
        matches!(self, Ack::Ok)
    }
}

/// Control channel commands
///
/// Sent by the server to instruct the client to perform actions.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum ControlChannelCmd {
    /// Create a new data channel
    CreateDataChannel,
    /// Heartbeat to keep connection alive
    HeartBeat,
}

/// Data channel commands
///
/// Sent by the server to indicate what type of forwarding to start.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum DataChannelCmd {
    /// Start TCP forwarding
    StartForwardTcp,
    /// Start UDP forwarding
    StartForwardUdp,
}

/// UDP packet length type
pub type UdpPacketLen = u16;

/// UDP header for encapsulated UDP traffic
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UdpHeader {
    /// Source address of the UDP packet
    pub from: SocketAddr,
    /// Length of the UDP data
    pub len: UdpPacketLen,
}

/// UDP traffic structure
///
/// Represents a UDP packet with its source address and data.
#[derive(Debug, Clone)]
pub struct UdpTraffic {
    /// Source address
    pub from: SocketAddr,
    /// Packet data
    pub data: Bytes,
}

impl UdpTraffic {
    /// Create new UDP traffic from address and data
    pub fn new(from: SocketAddr, data: Bytes) -> Self {
        UdpTraffic { from, data }
    }

    /// Get the header for this UDP traffic
    pub fn header(&self) -> UdpHeader {
        UdpHeader {
            from: self.from,
            len: self.data.len() as UdpPacketLen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_hello_control_channel() {
        let hello = Hello::control_channel("test-service");
        assert_eq!(hello.version(), CURRENT_PROTO_VERSION);
        assert_eq!(hello.digest().len(), HASH_WIDTH_IN_BYTES);
    }

    #[test]
    fn test_hello_data_channel() {
        let session_key = [0u8; HASH_WIDTH_IN_BYTES];
        let hello = Hello::data_channel(session_key);
        assert_eq!(hello.version(), CURRENT_PROTO_VERSION);
        assert_eq!(hello.digest(), &session_key);
    }

    #[test]
    fn test_auth_new() {
        let nonce = [1u8; HASH_WIDTH_IN_BYTES];
        let auth = Auth::new("secret-token", &nonce);
        assert_eq!(auth.0.len(), HASH_WIDTH_IN_BYTES);
    }

    #[test]
    fn test_ack_display() {
        assert_eq!(format!("{}", Ack::Ok), "Ok");
        assert_eq!(format!("{}", Ack::ServiceNotExist), "Service not exist");
        assert_eq!(format!("{}", Ack::AuthFailed), "Incorrect token");
    }

    #[test]
    fn test_ack_is_ok() {
        assert!(Ack::Ok.is_ok());
        assert!(!Ack::ServiceNotExist.is_ok());
        assert!(!Ack::AuthFailed.is_ok());
    }

    #[test]
    fn test_udp_traffic() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let data = Bytes::from_static(b"test data");
        let traffic = UdpTraffic::new(addr, data.clone());

        assert_eq!(traffic.from, addr);
        assert_eq!(traffic.data, data);

        let header = traffic.header();
        assert_eq!(header.from, addr);
        assert_eq!(header.len, 9); // "test data" length
    }
}
