//! UDP ASSOCIATE module for SOCKS5
//!
//! Handles UDP ASSOCIATE requests for relaying UDP traffic through the tunnel.

mod associate;
mod forwarder;
mod packet;

pub use associate::handle_udp_associate;
#[allow(unused_imports)]
pub use forwarder::UdpForwarder;
#[allow(unused_imports)]
pub use packet::{encode_udp_packet, parse_udp_packet, UdpPacket};

/// UDP relay state management
pub struct UdpRelay {
    /// Whether the relay is active
    active: bool,
}

impl UdpRelay {
    /// Create a new UDP relay
    pub fn new() -> Self {
        UdpRelay { active: false }
    }

    /// Check if the relay is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Default for UdpRelay {
    fn default() -> Self {
        Self::new()
    }
}
