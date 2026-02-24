//! UDP ASSOCIATE module for SOCKS5
//!
//! Handles UDP ASSOCIATE requests for relaying UDP traffic through the tunnel.
//! Also handles UDP data channel relay (when rathole sends `StartForwardUdp`).

mod associate;
mod packet;
mod relay;

pub use associate::handle_udp_associate;
pub use packet::{encode_udp_packet, parse_udp_packet, UdpPacket};
pub use relay::UdpRelay;
