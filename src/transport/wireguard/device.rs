//! Virtual network device for smoltcp.
//!
//! Implements the smoltcp [`Device`] trait using in-memory packet queues.
//! No TUN/TAP device is created — all IP packet exchange happens purely
//! in memory between [`VirtualDevice`], the smoltcp stack, and boringtun.
//!
//! Uses `Medium::Ip` so there is no Ethernet framing overhead.

use smoltcp::phy::{Checksum, ChecksumCapabilities, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use std::collections::VecDeque;

/// Default MTU for WireGuard tunnels (standard WG inner MTU).
pub const DEFAULT_WG_MTU: usize = 1420;

/// A virtual network device backed by in-memory packet queues.
///
/// Packets injected into `rx_queue` are delivered to smoltcp on the
/// next `poll()`.  Packets produced by smoltcp during `poll()` land
/// in `tx_queue` and can be drained for encryption by boringtun.
pub struct VirtualDevice {
    /// Packets waiting to be received by smoltcp (from boringtun).
    rx_queue: VecDeque<Vec<u8>>,
    /// Packets produced by smoltcp to be encrypted by boringtun.
    tx_queue: VecDeque<Vec<u8>>,
    /// Maximum transmission unit.
    mtu: usize,
}

impl VirtualDevice {
    /// Create a new virtual device with the given MTU.
    pub fn new(mtu: usize) -> Self {
        Self {
            rx_queue: VecDeque::with_capacity(64),
            tx_queue: VecDeque::with_capacity(64),
            mtu,
        }
    }

    /// Inject a decrypted IP packet into the receive queue.
    ///
    /// This packet will be processed by smoltcp on the next `poll()`.
    pub fn inject_rx(&mut self, packet: &[u8]) {
        self.rx_queue.push_back(packet.to_vec());
    }

    /// Drain all packets produced by smoltcp (outbound IP packets).
    ///
    /// The caller should encrypt these with boringtun and send via UDP.
    pub fn drain_tx(&mut self) -> impl Iterator<Item = Vec<u8>> + '_ {
        self.tx_queue.drain(..)
    }
}

impl Device for VirtualDevice {
    type RxToken<'a> = VirtualRxToken;
    type TxToken<'a> = VirtualTxToken<'a>;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Pop a packet from the RX queue and return tokens.
        // smoltcp requires both an RxToken (to consume the inbound packet)
        // and a TxToken (to possibly send a response) simultaneously.
        let buffer = self.rx_queue.pop_front()?;
        let rx = VirtualRxToken { buffer };
        let tx = VirtualTxToken {
            tx_queue: &mut self.tx_queue,
        };
        Some((rx, tx))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        // Always allow transmission — smoltcp writes into our TX queue.
        Some(VirtualTxToken {
            tx_queue: &mut self.tx_queue,
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = self.mtu;
        // Let smoltcp compute all checksums (we're in userspace, no
        // hardware offload).
        caps.checksum = ChecksumCapabilities::default();
        caps.checksum.ipv4 = Checksum::Both;
        caps.checksum.tcp = Checksum::Both;
        caps
    }
}

/// Receive token that owns an IP packet from the RX queue.
pub struct VirtualRxToken {
    buffer: Vec<u8>,
}

impl smoltcp::phy::RxToken for VirtualRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer)
    }
}

/// Transmit token that captures the produced packet into the TX queue.
pub struct VirtualTxToken<'a> {
    tx_queue: &'a mut VecDeque<Vec<u8>>,
}

impl<'a> smoltcp::phy::TxToken for VirtualTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0u8; len];
        let result = f(&mut buffer);
        self.tx_queue.push_back(buffer);
        result
    }
}

// Manual Debug impl because VecDeque<Vec<u8>> is not very informative.
impl std::fmt::Debug for VirtualDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualDevice")
            .field("mtu", &self.mtu)
            .field("rx_queue_len", &self.rx_queue.len())
            .field("tx_queue_len", &self.tx_queue.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_device() {
        let dev = VirtualDevice::new(1500);
        assert_eq!(dev.mtu, 1500);
        assert!(dev.rx_queue.is_empty());
        assert!(dev.tx_queue.is_empty());
    }

    #[test]
    fn test_default_mtu() {
        let dev = VirtualDevice::new(DEFAULT_WG_MTU);
        assert_eq!(dev.mtu, DEFAULT_WG_MTU);
    }

    #[test]
    fn test_inject_rx() {
        let mut dev = VirtualDevice::new(1420);
        assert_eq!(dev.rx_queue.len(), 0);

        dev.inject_rx(&[1, 2, 3, 4]);
        assert_eq!(dev.rx_queue.len(), 1);
        assert!(!dev.rx_queue.is_empty());
    }

    #[test]
    fn test_inject_rx_owned() {
        let mut dev = VirtualDevice::new(1420);
        dev.rx_queue.push_back(vec![5, 6, 7, 8]);
        assert_eq!(dev.rx_queue.len(), 1);
    }

    #[test]
    fn test_drain_tx() {
        let mut dev = VirtualDevice::new(1420);
        // Simulate smoltcp producing a packet by directly pushing to tx_queue
        dev.tx_queue.push_back(vec![10, 20, 30]);
        dev.tx_queue.push_back(vec![40, 50, 60]);

        assert!(!dev.tx_queue.is_empty());
        assert_eq!(dev.tx_queue.len(), 2);

        let packets: Vec<Vec<u8>> = dev.drain_tx().collect();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], vec![10, 20, 30]);
        assert_eq!(packets[1], vec![40, 50, 60]);
        assert!(dev.tx_queue.is_empty());
    }

    #[test]
    fn test_capabilities() {
        let dev = VirtualDevice::new(1420);
        let caps = dev.capabilities();
        assert_eq!(caps.medium, Medium::Ip);
        assert_eq!(caps.max_transmission_unit, 1420);
    }

    #[test]
    fn test_receive_empty() {
        let mut dev = VirtualDevice::new(1420);
        let result = dev.receive(Instant::ZERO);
        assert!(result.is_none());
    }

    #[test]
    fn test_receive_with_packet() {
        let mut dev = VirtualDevice::new(1420);
        dev.inject_rx(&[
            0x45, 0, 0, 20, 0, 0, 0, 0, 64, 6, 0, 0, 10, 0, 0, 2, 10, 0, 0, 1,
        ]);

        let result = dev.receive(Instant::ZERO);
        assert!(result.is_some());

        let (rx, _tx) = result.unwrap();
        // Consume the RX token
        use smoltcp::phy::RxToken;
        let data = rx.consume(|buf| buf.to_vec());
        assert_eq!(data.len(), 20);
        assert_eq!(data[0], 0x45); // IPv4 header
    }

    #[test]
    fn test_transmit_always_available() {
        let mut dev = VirtualDevice::new(1420);
        let result = dev.transmit(Instant::ZERO);
        assert!(result.is_some());

        // Consume the TX token
        use smoltcp::phy::TxToken;
        let tx = result.unwrap();
        tx.consume(10, |buf| {
            buf[0] = 0x45;
            buf[1] = 0x00;
        });

        assert_eq!(dev.tx_queue.len(), 1);
        let packets: Vec<_> = dev.drain_tx().collect();
        assert_eq!(packets[0][0], 0x45);
        assert_eq!(packets[0].len(), 10);
    }

    #[test]
    fn test_debug() {
        let dev = VirtualDevice::new(1420);
        let debug = format!("{:?}", dev);
        assert!(debug.contains("VirtualDevice"));
        assert!(debug.contains("mtu"));
    }
}
