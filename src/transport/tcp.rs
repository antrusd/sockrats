//! TCP transport implementation
//!
//! Provides plain TCP connections for the rathole protocol.

use super::{AddrMaybeCached, SocketOpts, StreamDyn, Transport, TransportDyn};
use crate::config::TransportConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::net::TcpStream;

/// TCP transport for plain connections
#[derive(Debug, Clone)]
pub struct TcpTransport {
    /// Socket options to apply to connections
    socket_opts: SocketOpts,
    /// Connection timeout
    connect_timeout: Duration,
}

impl TcpTransport {
    /// Create a new TCP transport with default options
    pub fn with_defaults() -> Self {
        TcpTransport {
            socket_opts: SocketOpts::default(),
            connect_timeout: Duration::from_secs(10),
        }
    }

    /// Set socket options
    pub fn with_socket_opts(mut self, opts: SocketOpts) -> Self {
        self.socket_opts = opts;
        self
    }

    /// Set connection timeout
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
}

#[async_trait]
impl Transport for TcpTransport {
    type Stream = TcpStream;

    fn new(config: &TransportConfig) -> Result<Self> {
        let socket_opts = SocketOpts::from_tcp_config(&config.tcp);
        Ok(TcpTransport {
            socket_opts,
            connect_timeout: Duration::from_secs(10),
        })
    }

    fn hint(conn: &Self::Stream, opts: SocketOpts) {
        if let Err(e) = opts.apply(conn) {
            tracing::warn!("Failed to apply socket options: {}", e);
        }
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        let resolved = addr.resolve().await?;

        let stream = tokio::time::timeout(self.connect_timeout, TcpStream::connect(resolved))
            .await
            .with_context(|| format!("Connection timeout to {}", addr.addr()))?
            .with_context(|| format!("Failed to connect to {}", addr.addr()))?;

        // Apply socket options
        self.socket_opts.apply(&stream)?;

        tracing::debug!("TCP connection established to {}", resolved);

        Ok(stream)
    }
}

#[async_trait]
impl TransportDyn for TcpTransport {
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>> {
        let stream = self.connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_transport_with_defaults() {
        let transport = TcpTransport::with_defaults();
        assert!(transport.socket_opts.nodelay);
        assert_eq!(transport.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_tcp_transport_with_socket_opts() {
        let opts = SocketOpts {
            nodelay: false,
            keepalive_secs: Some(60),
            keepalive_interval: Some(20),
        };
        let transport = TcpTransport::with_defaults().with_socket_opts(opts.clone());
        assert!(!transport.socket_opts.nodelay);
        assert_eq!(transport.socket_opts.keepalive_secs, Some(60));
    }

    #[test]
    fn test_tcp_transport_with_connect_timeout() {
        let transport = TcpTransport::with_defaults().with_connect_timeout(Duration::from_secs(30));
        assert_eq!(transport.connect_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_tcp_transport_new_from_config() {
        let config = TransportConfig::default();
        let transport = TcpTransport::new(&config).unwrap();
        assert!(transport.socket_opts.nodelay);
    }

    #[tokio::test]
    async fn test_tcp_transport_connect_localhost() {
        // This test requires a local server, so we just test that connection
        // attempts to a non-existent port fail appropriately
        let transport =
            TcpTransport::with_defaults().with_connect_timeout(Duration::from_millis(100));

        let addr = AddrMaybeCached::new("127.0.0.1:59999");
        let result = transport.connect(&addr).await;

        // Should fail since nothing is listening
        assert!(result.is_err());
    }
}
