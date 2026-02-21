//! UDP forwarder for SOCKS5
//!
//! Manages UDP forwarding sessions.

use crate::socks::types::TargetAddr;
use crate::socks::udp::packet::UdpPacket;
use anyhow::{Context, Result};
use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, warn};

/// Default UDP session timeout in seconds
#[allow(dead_code)]
const UDP_SESSION_TIMEOUT: Duration = Duration::from_secs(120);

/// UDP send queue size
#[allow(dead_code)]
const UDP_QUEUE_SIZE: usize = 256;

/// UDP forwarder that manages multiple UDP sessions
#[allow(dead_code)]
pub struct UdpForwarder {
    /// Active sessions keyed by target address
    sessions: Arc<RwLock<HashMap<SocketAddr, UdpSession>>>,
    /// Channel for outbound traffic
    outbound_tx: mpsc::Sender<UdpPacket>,
    /// Session timeout
    session_timeout: Duration,
}

/// A single UDP session
#[allow(dead_code)]
struct UdpSession {
    /// UDP socket for this session
    socket: Arc<UdpSocket>,
    /// Last activity time
    last_activity: Instant,
    /// Channel for sending data to this session
    send_tx: mpsc::Sender<Bytes>,
}

#[allow(dead_code)]
impl UdpForwarder {
    /// Create a new UDP forwarder
    pub fn new(outbound_tx: mpsc::Sender<UdpPacket>) -> Self {
        UdpForwarder {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            outbound_tx,
            session_timeout: UDP_SESSION_TIMEOUT,
        }
    }

    /// Set custom session timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = timeout;
        self
    }

    /// Forward a UDP packet to its destination
    pub async fn forward(&self, packet: UdpPacket) -> Result<()> {
        // Don't handle fragmented packets
        if packet.is_fragmented() {
            warn!("Fragmented UDP packets not supported");
            return Ok(());
        }

        // Resolve target address
        let target_addr = packet.addr.resolve().await
            .with_context(|| format!("Failed to resolve UDP target: {}", packet.addr))?;

        // Get or create session
        let session = self.get_or_create_session(target_addr, &packet.addr).await?;

        // Send data through session
        if let Err(e) = session.send_tx.send(packet.data).await {
            warn!("Failed to send UDP data: {}", e);
        }

        Ok(())
    }

    /// Get or create a session for the given target
    async fn get_or_create_session(
        &self,
        target_addr: SocketAddr,
        original_addr: &TargetAddr,
    ) -> Result<UdpSession> {
        // Check for existing session
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&target_addr) {
                return Ok(UdpSession {
                    socket: session.socket.clone(),
                    last_activity: Instant::now(),
                    send_tx: session.send_tx.clone(),
                });
            }
        }

        // Create new session
        let socket = UdpSocket::bind("0.0.0.0:0").await
            .with_context(|| "Failed to bind UDP socket")?;
        socket.connect(target_addr).await
            .with_context(|| format!("Failed to connect UDP socket to {}", target_addr))?;

        let socket = Arc::new(socket);
        let (send_tx, send_rx) = mpsc::channel(UDP_QUEUE_SIZE);

        let session = UdpSession {
            socket: socket.clone(),
            last_activity: Instant::now(),
            send_tx: send_tx.clone(),
        };

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(target_addr, UdpSession {
                socket: socket.clone(),
                last_activity: Instant::now(),
                send_tx,
            });
        }

        // Spawn sender task
        let socket_send = socket.clone();
        tokio::spawn(async move {
            run_sender(socket_send, send_rx).await;
        });

        // Spawn receiver task
        let sessions_ref = self.sessions.clone();
        let outbound_tx = self.outbound_tx.clone();
        let addr = original_addr.clone();
        let timeout = self.session_timeout;
        tokio::spawn(async move {
            run_receiver(socket, target_addr, addr, outbound_tx, sessions_ref, timeout).await;
        });

        Ok(session)
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();

        sessions.retain(|addr, session| {
            let expired = now.duration_since(session.last_activity) > self.session_timeout;
            if expired {
                debug!("Cleaning up expired UDP session for {}", addr);
            }
            !expired
        });
    }
}

/// Run the sender loop for a UDP session
#[allow(dead_code)]
async fn run_sender(socket: Arc<UdpSocket>, mut rx: mpsc::Receiver<Bytes>) {
    while let Some(data) = rx.recv().await {
        if let Err(e) = socket.send(&data).await {
            warn!("UDP send error: {}", e);
            break;
        }
    }
    debug!("UDP sender terminated");
}

/// Run the receiver loop for a UDP session
#[allow(dead_code)]
async fn run_receiver(
    socket: Arc<UdpSocket>,
    target_addr: SocketAddr,
    addr: TargetAddr,
    outbound_tx: mpsc::Sender<UdpPacket>,
    sessions: Arc<RwLock<HashMap<SocketAddr, UdpSession>>>,
    timeout: Duration,
) {
    let mut buf = vec![0u8; 65535];

    loop {
        tokio::select! {
            result = socket.recv(&mut buf) => {
                match result {
                    Ok(len) => {
                        let data = Bytes::copy_from_slice(&buf[..len]);
                        let packet = UdpPacket::new(addr.clone(), data);

                        if outbound_tx.send(packet).await.is_err() {
                            break;
                        }

                        // Update last activity
                        if let Some(session) = sessions.write().await.get_mut(&target_addr) {
                            session.last_activity = Instant::now();
                        }
                    }
                    Err(e) => {
                        debug!("UDP recv error: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                debug!("UDP session timeout for {}", target_addr);
                break;
            }
        }
    }

    // Remove session
    sessions.write().await.remove(&target_addr);
    debug!("UDP receiver terminated for {}", target_addr);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_udp_forwarder_creation() {
        let (tx, _rx) = mpsc::channel(16);
        let forwarder = UdpForwarder::new(tx);
        assert_eq!(forwarder.session_timeout, UDP_SESSION_TIMEOUT);
    }

    #[tokio::test]
    async fn test_udp_forwarder_with_timeout() {
        let (tx, _rx) = mpsc::channel(16);
        let forwarder = UdpForwarder::new(tx)
            .with_timeout(Duration::from_secs(60));
        assert_eq!(forwarder.session_timeout, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let (tx, _rx) = mpsc::channel(16);
        let forwarder = UdpForwarder::new(tx)
            .with_timeout(Duration::from_millis(10));

        // No sessions to clean up
        forwarder.cleanup_expired().await;

        let sessions = forwarder.sessions.read().await;
        assert!(sessions.is_empty());
    }
}
