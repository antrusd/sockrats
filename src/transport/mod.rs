//! Transport module for Sockrats
//!
//! This module provides the transport layer abstraction and implementations
//! for different protocols (TCP, Noise).

mod addr;
mod noise;
mod tcp;
#[cfg(feature = "wireguard")]
pub mod wireguard;

pub use addr::AddrMaybeCached;
pub use noise::NoiseTransport;
pub use tcp::TcpTransport;
#[cfg(feature = "wireguard")]
pub use wireguard::WireguardTransport;

use crate::config::{TcpConfig, TransportConfig, TransportType};
use anyhow::Result;
use async_trait::async_trait;
use std::fmt::Debug;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// Socket options for configuring connections
#[derive(Debug, Clone)]
pub struct SocketOpts {
    /// Enable TCP_NODELAY
    pub nodelay: bool,
    /// TCP keepalive timeout
    pub keepalive_secs: Option<u64>,
    /// TCP keepalive interval
    pub keepalive_interval: Option<u64>,
}

impl Default for SocketOpts {
    fn default() -> Self {
        SocketOpts {
            nodelay: true,
            keepalive_secs: Some(20),
            keepalive_interval: Some(8),
        }
    }
}

impl SocketOpts {
    /// Create socket options for control channel (longer keepalive)
    pub fn for_control_channel() -> Self {
        SocketOpts {
            nodelay: true,
            keepalive_secs: Some(30),
            keepalive_interval: Some(10),
        }
    }

    /// Create socket options for data channel (optimized for throughput)
    pub fn for_data_channel() -> Self {
        SocketOpts {
            nodelay: true,
            keepalive_secs: Some(20),
            keepalive_interval: Some(8),
        }
    }

    /// Create socket options from TCP config
    pub fn from_tcp_config(config: &TcpConfig) -> Self {
        SocketOpts {
            nodelay: config.nodelay,
            keepalive_secs: Some(config.keepalive_secs),
            keepalive_interval: Some(config.keepalive_interval),
        }
    }

    /// Apply socket options to a TCP stream
    pub fn apply(&self, stream: &TcpStream) -> std::io::Result<()> {
        stream.set_nodelay(self.nodelay)?;

        if let (Some(timeout), Some(interval)) = (self.keepalive_secs, self.keepalive_interval) {
            let socket = socket2::SockRef::from(stream);
            let keepalive = socket2::TcpKeepalive::new()
                .with_time(Duration::from_secs(timeout))
                .with_interval(Duration::from_secs(interval));
            socket.set_tcp_keepalive(&keepalive)?;
        }

        Ok(())
    }
}

/// Transport trait for different connection types
///
/// This trait defines the interface for all transport implementations.
/// Implementations must be able to connect to remote addresses and
/// return streams that implement AsyncRead + AsyncWrite.
#[async_trait]
pub trait Transport: Debug + Send + Sync + 'static {
    /// The stream type produced by this transport
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + Sync + Debug + 'static;

    /// Create a new transport instance from configuration
    fn new(config: &TransportConfig) -> Result<Self>
    where
        Self: Sized;

    /// Apply socket hints/options to a connection
    fn hint(conn: &Self::Stream, opts: SocketOpts);

    /// Connect to a remote address
    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream>;
}

/// Create a transport based on configuration
pub fn create_transport(config: &TransportConfig) -> Result<Box<dyn TransportDyn>> {
    match config.transport_type {
        TransportType::Tcp => {
            let transport = TcpTransport::new(config)?;
            Ok(Box::new(transport))
        }
        TransportType::Noise => {
            let transport = NoiseTransport::new(config)?;
            Ok(Box::new(transport))
        }
    }
}

/// Dynamic transport trait for boxed transports
#[async_trait]
pub trait TransportDyn: Debug + Send + Sync {
    /// Connect to a remote address and return a boxed stream
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>>;
}

/// Dynamic stream trait for boxed streams
pub trait StreamDyn: AsyncRead + AsyncWrite + Unpin + Send + Sync + Debug {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync + Debug> StreamDyn for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_opts_default() {
        let opts = SocketOpts::default();
        assert!(opts.nodelay);
        assert_eq!(opts.keepalive_secs, Some(20));
        assert_eq!(opts.keepalive_interval, Some(8));
    }

    #[test]
    fn test_socket_opts_for_control_channel() {
        let opts = SocketOpts::for_control_channel();
        assert!(opts.nodelay);
        assert_eq!(opts.keepalive_secs, Some(30));
    }

    #[test]
    fn test_socket_opts_for_data_channel() {
        let opts = SocketOpts::for_data_channel();
        assert!(opts.nodelay);
        assert_eq!(opts.keepalive_secs, Some(20));
    }

    #[test]
    fn test_socket_opts_from_tcp_config() {
        let config = TcpConfig {
            nodelay: false,
            keepalive_secs: 60,
            keepalive_interval: 15,
        };
        let opts = SocketOpts::from_tcp_config(&config);
        assert!(!opts.nodelay);
        assert_eq!(opts.keepalive_secs, Some(60));
        assert_eq!(opts.keepalive_interval, Some(15));
    }
}
