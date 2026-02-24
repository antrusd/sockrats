//! Central event loop orchestrating UDP, boringtun, and smoltcp.
//!
//! [`WgEventLoop`] runs as a background `tokio::spawn` task and
//! coordinates all packet flow between the real network (UDP socket),
//! the WireGuard crypto layer (boringtun), and the virtual TCP/IP
//! stack (smoltcp).

use super::config::WireguardConfig;
use super::device::DEFAULT_WG_MTU;
use super::stack::VirtualStack;
use super::stream::{StreamChannelPair, StreamMessage, WireguardStream};
use super::tunnel::{DecapResult, EncapResult, TunnelHandle};
use anyhow::{Context, Result};
use bytes::Bytes;
use smoltcp::iface::SocketHandle;
use smoltcp::time::Instant as SmolInstant;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time;
use tracing::{debug, error, info, trace, warn};

/// Interval between boringtun timer ticks (ms).
const TIMER_TICK_MS: u64 = 250;

/// Maximum number of concurrent virtual TCP connections.
const MAX_STREAMS: usize = 256;

/// Size of the connect-request channel.
const CONNECT_CHANNEL_SIZE: usize = 64;

/// Size of the UDP receive buffer.
const UDP_BUF_SIZE: usize = 65536;

/// Size of the buffer for reading from smoltcp sockets.
const RECV_BUF_SIZE: usize = 8192;

/// A request to create a new virtual TCP connection.
struct ConnectRequest {
    remote_addr: SocketAddr,
    response_tx: oneshot::Sender<Result<WireguardStream>>,
}

/// A pending connection waiting for the TCP handshake to complete.
struct PendingConnect {
    handle: SocketHandle,
    stream_id: u32,
    response_tx: oneshot::Sender<Result<WireguardStream>>,
    channels: StreamChannelPair,
    deadline: tokio::time::Instant,
}

/// Handle for communicating with the event loop from external code.
pub struct WgEventLoop {
    /// Channel to submit new connection requests.
    connect_tx: mpsc::Sender<ConnectRequest>,
    /// Background task handle.
    task_handle: tokio::task::JoinHandle<()>,
}

impl WgEventLoop {
    /// Start the event loop in a background task.
    ///
    /// Binds a UDP socket, creates the boringtun tunnel and smoltcp
    /// stack, then spawns the main loop.
    pub async fn start(config: &WireguardConfig) -> Result<Self> {
        // Create the tunnel handle (boringtun).
        let mut tunnel = TunnelHandle::new(config).context("Failed to create WireGuard tunnel")?;

        // Resolve the peer endpoint.
        let peer_endpoint = config
            .parse_peer_endpoint()
            .context("Failed to resolve WireGuard peer endpoint")?;

        // Create the virtual stack (smoltcp).
        let (client_ip, prefix_len) = config.parse_address()?;
        let stack = VirtualStack::new(client_ip, prefix_len, DEFAULT_WG_MTU)
            .context("Failed to create virtual TCP/IP stack")?;

        // Bind a UDP socket (ephemeral port).
        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Failed to bind WireGuard UDP socket")?;
        let local_udp = udp_socket.local_addr()?;
        info!(
            "WireGuard UDP socket bound on {} -> peer {}",
            local_udp, peer_endpoint
        );

        // Initiate the WireGuard handshake immediately.
        if let Some(init_pkt) = tunnel.force_handshake() {
            udp_socket
                .send_to(&init_pkt, peer_endpoint)
                .await
                .context("Failed to send initial WG handshake")?;
            debug!("Sent initial WireGuard handshake to {}", peer_endpoint);
        }

        // Create channels.
        let (connect_tx, connect_rx) = mpsc::channel(CONNECT_CHANNEL_SIZE);

        // Spawn the event loop task.
        let task_handle = tokio::spawn(async move {
            let mut inner = EventLoopInner {
                udp_socket,
                tunnel,
                stack,
                streams: HashMap::new(),
                connect_rx,
                pending_connects: Vec::new(),
                peer_endpoint,
                next_stream_id: 1,
            };
            if let Err(e) = inner.run().await {
                error!("WireGuard event loop exited with error: {:#}", e);
            }
        });

        Ok(Self {
            connect_tx,
            task_handle,
        })
    }

    /// Create a new virtual TCP connection through the WireGuard tunnel.
    pub async fn connect(
        &self,
        addr: &crate::transport::AddrMaybeCached,
        timeout: Duration,
    ) -> Result<WireguardStream> {
        let remote_addr = addr.resolve().await?;

        let (response_tx, response_rx) = oneshot::channel();
        let request = ConnectRequest {
            remote_addr,
            response_tx,
        };

        self.connect_tx
            .send(request)
            .await
            .map_err(|_| anyhow::anyhow!("WireGuard event loop is shut down"))?;

        tokio::time::timeout(timeout, response_rx)
            .await
            .with_context(|| {
                format!(
                    "Timeout waiting for WireGuard TCP connection to {}",
                    remote_addr
                )
            })?
            .with_context(|| "Event loop dropped connection response")?
    }

    /// Check if the event loop is still running.
    pub fn is_running(&self) -> bool {
        !self.task_handle.is_finished()
    }
}

impl std::fmt::Debug for WgEventLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgEventLoop")
            .field("running", &self.is_running())
            .finish()
    }
}

/// Internal state of the event loop task.
struct EventLoopInner {
    udp_socket: UdpSocket,
    tunnel: TunnelHandle,
    stack: VirtualStack,
    streams: HashMap<SocketHandle, StreamChannelPair>,
    connect_rx: mpsc::Receiver<ConnectRequest>,
    pending_connects: Vec<PendingConnect>,
    peer_endpoint: SocketAddr,
    next_stream_id: u32,
}

impl EventLoopInner {
    /// Main event loop.
    async fn run(&mut self) -> Result<()> {
        let mut udp_buf = vec![0u8; UDP_BUF_SIZE];
        let mut recv_buf = [0u8; RECV_BUF_SIZE];
        let mut timer = time::interval(Duration::from_millis(TIMER_TICK_MS));
        timer.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        info!("WireGuard event loop started");

        loop {
            // Collect stream outbound messages.
            // We do a non-blocking check of all streams each iteration.
            self.process_stream_outbound();

            tokio::select! {
                // 1. Receive encrypted UDP datagrams from the real network.
                result = self.udp_socket.recv_from(&mut udp_buf) => {
                    match result {
                        Ok((n, _src)) => {
                            self.handle_udp_rx(&udp_buf[..n]).await;
                        }
                        Err(e) => {
                            warn!("UDP recv error: {}", e);
                        }
                    }
                }

                // 2. Timer tick — update boringtun timers.
                _ = timer.tick() => {
                    self.handle_timer_tick().await;
                }

                // 3. New connection requests.
                Some(req) = self.connect_rx.recv() => {
                    self.handle_connect_request(req);
                }
            }

            // After any event, run the packet pipeline.
            self.run_pipeline(&mut recv_buf).await;

            // Check pending connects.
            self.check_pending_connects();

            // Clean up closed streams.
            self.cleanup_closed_streams();
        }
    }

    /// Handle an incoming encrypted UDP datagram.
    async fn handle_udp_rx(&mut self, data: &[u8]) {
        // Decapsulate — may produce IP packets or control responses.
        // We must copy outbound data before calling send_udp() because
        // DecapResult borrows from self.tunnel, conflicting with &self.
        let result = self.tunnel.decapsulate(data);
        match result {
            DecapResult::IpPacket(pkt) => {
                // inject_packet borrows self.stack (disjoint from self.tunnel) — OK.
                self.stack.inject_packet(pkt);
            }
            DecapResult::SendToNetwork(pkt) => {
                let pkt = pkt.to_vec();
                self.send_udp(&pkt).await;
                // Continue flushing — boringtun may have queued data.
                self.flush_decapsulate().await;
            }
            DecapResult::Done => {}
        }
    }

    /// Flush queued packets after a handshake response.
    async fn flush_decapsulate(&mut self) {
        loop {
            let result = self.tunnel.decapsulate_flush();
            match result {
                DecapResult::IpPacket(pkt) => {
                    self.stack.inject_packet(pkt);
                }
                DecapResult::SendToNetwork(pkt) => {
                    let pkt = pkt.to_vec();
                    self.send_udp(&pkt).await;
                }
                DecapResult::Done => break,
            }
        }
    }

    /// Handle a boringtun timer tick.
    async fn handle_timer_tick(&mut self) {
        let packets = self.tunnel.update_timers();
        for pkt in &packets {
            self.send_udp(pkt).await;
        }
    }

    /// Handle a new connection request from `WireguardTransport::connect()`.
    fn handle_connect_request(&mut self, req: ConnectRequest) {
        if self.streams.len() >= MAX_STREAMS {
            let _ = req.response_tx.send(Err(anyhow::anyhow!(
                "Maximum concurrent WireGuard streams ({}) exceeded",
                MAX_STREAMS
            )));
            return;
        }

        let remote_ip = match req.remote_addr.ip() {
            std::net::IpAddr::V4(ip) => ip,
            std::net::IpAddr::V6(_) => {
                let _ = req.response_tx.send(Err(anyhow::anyhow!(
                    "IPv6 not supported in WireGuard virtual stack"
                )));
                return;
            }
        };

        match self.stack.connect_tcp(remote_ip, req.remote_addr.port()) {
            Ok(handle) => {
                let stream_id = self.next_stream_id;
                self.next_stream_id += 1;

                let (stream, channels) = WireguardStream::new_pair(stream_id);

                let pending = PendingConnect {
                    handle,
                    stream_id,
                    response_tx: req.response_tx,
                    channels,
                    deadline: tokio::time::Instant::now() + Duration::from_secs(10),
                };
                self.pending_connects.push(pending);

                debug!(
                    "Virtual TCP connect initiated: stream_id={}, target={}",
                    stream_id, req.remote_addr
                );

                // We DON'T send the stream back yet — we wait for the TCP
                // handshake to complete (Established state).
                // The stream will be sent from check_pending_connects().
                let _ = stream; // consumed later via new_pair re-creation
            }
            Err(e) => {
                let _ = req.response_tx.send(Err(e));
            }
        }
    }

    /// Process outbound data from all active WireguardStreams (non-blocking).
    fn process_stream_outbound(&mut self) {
        let handles: Vec<SocketHandle> = self.streams.keys().copied().collect();
        for handle in handles {
            if let Some(channels) = self.streams.get_mut(&handle) {
                // Drain all available messages without blocking.
                loop {
                    match channels.outbound_rx.try_recv() {
                        Ok(StreamMessage::Data(data)) => {
                            if let Err(e) = self.stack.tcp_send(handle, &data) {
                                trace!("tcp_send failed for {:?}: {}", handle, e);
                            }
                        }
                        Ok(StreamMessage::Flush) => {
                            // Poll will be called after this method.
                        }
                        Ok(StreamMessage::Close) => {
                            self.stack.close_tcp(handle);
                        }
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            // Stream was dropped — close the socket.
                            self.stack.close_tcp(handle);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Run the full packet pipeline: poll smoltcp, encrypt outbound
    /// packets, and deliver inbound data to streams.
    async fn run_pipeline(&mut self, recv_buf: &mut [u8]) {
        // Poll smoltcp to process packets between device and sockets.
        self.stack.poll(SmolInstant::now());

        // Encrypt and send outbound IP packets from smoltcp.
        let tx_packets = self.stack.drain_tx_packets();
        for ip_pkt in &tx_packets {
            // Copy outbound data before send_udp() — EncapResult borrows
            // from self.tunnel which conflicts with &self in send_udp().
            let result = self.tunnel.encapsulate(ip_pkt);
            match result {
                EncapResult::Packet(encrypted) => {
                    let encrypted = encrypted.to_vec();
                    self.send_udp(&encrypted).await;
                }
                EncapResult::HandshakeInit(pkt) => {
                    let pkt = pkt.to_vec();
                    self.send_udp(&pkt).await;
                }
                EncapResult::Done => {}
            }
        }

        // Deliver received data from smoltcp sockets to streams.
        let handles: Vec<SocketHandle> = self.streams.keys().copied().collect();
        for handle in handles {
            if !self.stack.tcp_can_recv(handle) {
                continue;
            }
            match self.stack.tcp_recv(handle, recv_buf) {
                Ok(n) if n > 0 => {
                    if let Some(channels) = self.streams.get(&handle) {
                        let data = Bytes::copy_from_slice(&recv_buf[..n]);
                        if channels.inbound_tx.try_send(data).is_err() {
                            warn!("Stream inbound channel full/closed for {:?}", handle);
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    trace!("tcp_recv error for {:?}: {}", handle, e);
                }
            }
        }
    }

    /// Check pending TCP connections for completion.
    fn check_pending_connects(&mut self) {
        let now = tokio::time::Instant::now();
        let mut completed = Vec::new();

        for (i, pending) in self.pending_connects.iter().enumerate() {
            if self.stack.is_tcp_connected(pending.handle) {
                completed.push((i, true));
            } else if self.stack.is_tcp_closed(pending.handle) || now >= pending.deadline {
                completed.push((i, false));
            }
        }

        // Process in reverse order to preserve indices.
        for (i, success) in completed.into_iter().rev() {
            let pending = self.pending_connects.remove(i);
            if success {
                // Re-create the stream pair since we need to send
                // the stream to the caller.
                let (stream, channels) = WireguardStream::new_pair(pending.stream_id);

                // Register the channels in our active streams map.
                self.streams.insert(pending.handle, channels);

                debug!(
                    "Virtual TCP connected: stream_id={}, handle={:?}",
                    pending.stream_id, pending.handle
                );

                let _ = pending.response_tx.send(Ok(stream));
            } else {
                let state = self.stack.tcp_state_str(pending.handle);
                warn!(
                    "Virtual TCP connect failed: stream_id={}, state={}",
                    pending.stream_id, state
                );
                self.stack.abort_tcp(pending.handle);
                let _ = pending.response_tx.send(Err(anyhow::anyhow!(
                    "WireGuard virtual TCP connection failed (state={})",
                    state
                )));
            }
            // Drop the original pending channels (unused).
            drop(pending.channels);
        }
    }

    /// Clean up streams whose virtual TCP sockets have closed.
    fn cleanup_closed_streams(&mut self) {
        let closed: Vec<SocketHandle> = self
            .streams
            .keys()
            .filter(|h| self.stack.is_tcp_closed(**h))
            .copied()
            .collect();

        for handle in closed {
            if let Some(channels) = self.streams.remove(&handle) {
                debug!("Cleaning up closed stream for {:?}", handle);
                // Send empty Bytes to signal EOF to the WireguardStream.
                let _ = channels.inbound_tx.try_send(Bytes::new());
                drop(channels);
            }
            self.stack.remove_tcp(handle);
        }
    }

    /// Send a packet to the WireGuard peer via UDP.
    async fn send_udp(&self, data: &[u8]) {
        if let Err(e) = self.udp_socket.send_to(data, self.peer_endpoint).await {
            warn!("UDP send error: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_channel_size() {
        assert!(CONNECT_CHANNEL_SIZE > 0);
        assert!(MAX_STREAMS > 0);
    }

    #[test]
    fn test_timer_tick_interval() {
        assert_eq!(TIMER_TICK_MS, 250);
    }

    // Integration-level tests for the event loop require a real
    // WireGuard peer and are deferred to the integration test suite.
    // Unit-level behaviour is covered by the component tests in
    // tunnel.rs, stack.rs, device.rs, and stream.rs.
}
