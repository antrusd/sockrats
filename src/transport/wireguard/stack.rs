//! Virtual TCP/IP stack built on smoltcp.
//!
//! [`VirtualStack`] manages a smoltcp [`Interface`] and [`SocketSet`],
//! providing virtual TCP socket lifecycle (connect, send, recv, close)
//! without touching the OS kernel networking layer.

use super::device::VirtualDevice;
use anyhow::{bail, Context, Result};
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr};
use std::net::Ipv4Addr;
use tracing::{debug, trace};

/// Default TCP socket receive buffer size.
const TCP_RX_BUF_SIZE: usize = 65536;

/// Default TCP socket transmit buffer size.
const TCP_TX_BUF_SIZE: usize = 65536;

/// Starting ephemeral port for virtual TCP connections.
const EPHEMERAL_PORT_START: u16 = 49152;

/// Upper bound for ephemeral ports.
const EPHEMERAL_PORT_END: u16 = 65535;

/// Virtual TCP/IP stack managing smoltcp internals.
pub struct VirtualStack {
    /// The smoltcp network interface.
    iface: Interface,
    /// Socket collection owned by smoltcp.
    sockets: SocketSet<'static>,
    /// The virtual device that exchanges IP packets with boringtun.
    device: VirtualDevice,
    /// Next ephemeral port to assign.
    next_port: u16,
}

impl VirtualStack {
    /// Create a new virtual stack with the given client IP, prefix length, and MTU.
    pub fn new(client_ip: Ipv4Addr, prefix_len: u8, mtu: usize) -> Result<Self> {
        let mut device = VirtualDevice::new(mtu);

        // Configure smoltcp interface in IP mode (no Ethernet framing)
        let config = Config::new(HardwareAddress::Ip);
        let mut iface = Interface::new(config, &mut device, Instant::now());

        // Assign the client's virtual IP with the configured prefix
        let ip_addr = IpCidr::new(IpAddress::Ipv4(client_ip), prefix_len);
        iface.update_ip_addrs(|addrs| {
            addrs.push(ip_addr).ok();
        });

        debug!("Virtual stack created: ip={}, mtu={}", client_ip, mtu);

        Ok(Self {
            iface,
            sockets: SocketSet::new(Vec::new()),
            device,
            next_port: EPHEMERAL_PORT_START,
        })
    }

    /// Create a new virtual TCP socket and initiate a connection.
    ///
    /// Returns the socket handle used to reference this connection.
    pub fn connect_tcp(&mut self, remote_ip: Ipv4Addr, remote_port: u16) -> Result<SocketHandle> {
        let local_port = self.allocate_port();

        let tcp_rx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_RX_BUF_SIZE]);
        let tcp_tx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_TX_BUF_SIZE]);
        let mut socket = tcp::Socket::new(tcp_rx_buf, tcp_tx_buf);

        let remote = (IpAddress::Ipv4(remote_ip), remote_port);
        let local_endpoint = local_port;

        socket
            .connect(self.iface.context(), remote, local_endpoint)
            .with_context(|| format!("smoltcp connect failed to {}:{}", remote_ip, remote_port))?;

        let handle = self.sockets.add(socket);

        debug!(
            "Virtual TCP: connecting local:{} -> {}:{}  (handle={:?})",
            local_port, remote_ip, remote_port, handle
        );

        Ok(handle)
    }

    /// Poll the interface — processes packets between device and sockets.
    pub fn poll(&mut self, timestamp: Instant) {
        let _ = self
            .iface
            .poll(timestamp, &mut self.device, &mut self.sockets);
    }

    /// Check if a TCP socket's three-way handshake is complete.
    pub fn is_tcp_connected(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket<'_>>(handle);
        socket.state() == tcp::State::Established
    }

    /// Check if a TCP socket is closed, closing, or in an error state.
    pub fn is_tcp_closed(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket<'_>>(handle);
        matches!(
            socket.state(),
            tcp::State::Closed | tcp::State::Closing | tcp::State::TimeWait | tcp::State::LastAck
        )
    }

    /// Check if a TCP socket has data available to read.
    pub fn tcp_can_recv(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket<'_>>(handle);
        socket.can_recv()
    }

    /// Write application data into a virtual TCP socket's send buffer.
    ///
    /// Returns the number of bytes actually accepted by the socket.
    pub fn tcp_send(&mut self, handle: SocketHandle, data: &[u8]) -> Result<usize> {
        let socket = self.sockets.get_mut::<tcp::Socket<'_>>(handle);
        if !socket.can_send() {
            bail!("TCP socket not ready to send (state={:?})", socket.state());
        }
        let n = socket
            .send_slice(data)
            .with_context(|| "smoltcp send_slice failed")?;
        trace!("tcp_send: {} bytes to {:?}", n, handle);
        Ok(n)
    }

    /// Read application data from a virtual TCP socket's receive buffer.
    ///
    /// Returns the number of bytes read.
    pub fn tcp_recv(&mut self, handle: SocketHandle, buf: &mut [u8]) -> Result<usize> {
        let socket = self.sockets.get_mut::<tcp::Socket<'_>>(handle);
        if !socket.can_recv() {
            return Ok(0);
        }
        let n = socket
            .recv_slice(buf)
            .with_context(|| "smoltcp recv_slice failed")?;
        trace!("tcp_recv: {} bytes from {:?}", n, handle);
        Ok(n)
    }

    /// Inject a decrypted IP packet into the virtual device's RX queue.
    pub fn inject_packet(&mut self, packet: &[u8]) {
        self.device.inject_rx(packet);
    }

    /// Drain all outbound IP packets from the virtual device's TX queue.
    pub fn drain_tx_packets(&mut self) -> Vec<Vec<u8>> {
        self.device.drain_tx().collect()
    }

    /// Gracefully close a TCP socket (sends FIN).
    pub fn close_tcp(&mut self, handle: SocketHandle) {
        let socket = self.sockets.get_mut::<tcp::Socket<'_>>(handle);
        debug!(
            "Closing virtual TCP socket {:?} (state={:?})",
            handle,
            socket.state()
        );
        socket.close();
    }

    /// Abort a TCP socket (sends RST).
    pub fn abort_tcp(&mut self, handle: SocketHandle) {
        let socket = self.sockets.get_mut::<tcp::Socket<'_>>(handle);
        debug!("Aborting virtual TCP socket {:?}", handle);
        socket.abort();
    }

    /// Remove a socket from the set (should only be done after close).
    pub fn remove_tcp(&mut self, handle: SocketHandle) {
        self.sockets.remove(handle);
    }

    /// Get the TCP socket state as a string (for logging).
    pub fn tcp_state_str(&self, handle: SocketHandle) -> &'static str {
        let socket = self.sockets.get::<tcp::Socket<'_>>(handle);
        match socket.state() {
            tcp::State::Closed => "Closed",
            tcp::State::Listen => "Listen",
            tcp::State::SynSent => "SynSent",
            tcp::State::SynReceived => "SynReceived",
            tcp::State::Established => "Established",
            tcp::State::FinWait1 => "FinWait1",
            tcp::State::FinWait2 => "FinWait2",
            tcp::State::CloseWait => "CloseWait",
            tcp::State::Closing => "Closing",
            tcp::State::LastAck => "LastAck",
            tcp::State::TimeWait => "TimeWait",
        }
    }

    /// Allocate the next ephemeral port, wrapping around when exhausted.
    fn allocate_port(&mut self) -> u16 {
        let port = self.next_port;
        self.next_port = if self.next_port == EPHEMERAL_PORT_END {
            EPHEMERAL_PORT_START
        } else {
            self.next_port + 1
        };
        port
    }
}

impl std::fmt::Debug for VirtualStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualStack")
            .field("next_port", &self.next_port)
            .field("device", &self.device)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stack() {
        let stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420);
        assert!(stack.is_ok());
    }

    #[test]
    fn test_allocate_ports() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let p1 = stack.allocate_port();
        let p2 = stack.allocate_port();
        assert_eq!(p1, EPHEMERAL_PORT_START);
        assert_eq!(p2, EPHEMERAL_PORT_START + 1);
    }

    #[test]
    fn test_port_wraps() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        stack.next_port = EPHEMERAL_PORT_END;
        let p1 = stack.allocate_port();
        let p2 = stack.allocate_port();
        assert_eq!(p1, EPHEMERAL_PORT_END);
        assert_eq!(p2, EPHEMERAL_PORT_START);
    }

    #[test]
    fn test_connect_tcp() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let handle = stack.connect_tcp(Ipv4Addr::new(10, 0, 0, 1), 2333);
        assert!(handle.is_ok());
    }

    #[test]
    fn test_tcp_initial_state() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let handle = stack.connect_tcp(Ipv4Addr::new(10, 0, 0, 1), 2333).unwrap();

        // After connect, socket should be in SynSent state
        assert!(!stack.is_tcp_connected(handle));
        assert!(!stack.is_tcp_closed(handle));
        assert_eq!(stack.tcp_state_str(handle), "SynSent");
    }

    #[test]
    fn test_poll_produces_syn() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let _handle = stack.connect_tcp(Ipv4Addr::new(10, 0, 0, 1), 2333).unwrap();

        // Poll should produce a SYN packet
        stack.poll(Instant::now());

        let packets = stack.drain_tx_packets();
        assert!(
            !packets.is_empty(),
            "Expected SYN packet after connect + poll"
        );

        // Verify it's an IPv4 packet (first nibble = 4)
        let pkt = &packets[0];
        assert!(!pkt.is_empty());
        assert_eq!(pkt[0] >> 4, 4, "Expected IPv4 packet");
    }

    #[test]
    fn test_inject_packet() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();

        // Inject a minimal IPv4 packet and verify poll processes it
        // (no panic = inject succeeded).
        stack.inject_packet(&[0x45, 0, 0, 20]);
        stack.poll(Instant::now());
    }

    #[test]
    fn test_close_tcp() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let handle = stack.connect_tcp(Ipv4Addr::new(10, 0, 0, 1), 2333).unwrap();

        stack.close_tcp(handle);
        // After close, state transitions (may need poll to process)
    }

    #[test]
    fn test_abort_tcp() {
        let mut stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let handle = stack.connect_tcp(Ipv4Addr::new(10, 0, 0, 1), 2333).unwrap();

        stack.abort_tcp(handle);
        // After abort, socket should be closed
        assert!(stack.is_tcp_closed(handle));
    }

    #[test]
    fn test_debug_impl() {
        let stack = VirtualStack::new(Ipv4Addr::new(10, 0, 0, 2), 24, 1420).unwrap();
        let debug = format!("{:?}", stack);
        assert!(debug.contains("VirtualStack"));
    }
}
