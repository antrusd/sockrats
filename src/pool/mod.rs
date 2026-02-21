//! Connection pool module for SocksRat
//!
//! This module provides connection pooling for data channels,
//! allowing pre-established connections to reduce latency.

mod channel;
mod guard;
mod manager;
mod tcp_pool;

pub use channel::PooledChannel;
pub use guard::PooledChannelGuard;
pub use manager::PoolManager;
pub use tcp_pool::TcpChannelPool;

use crate::config::PoolConfig;
use crate::protocol::Digest;
use crate::transport::{AddrMaybeCached, Transport};
use anyhow::Result;
use std::sync::Arc;

/// Channel type indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    /// TCP data channel
    Tcp,
    /// UDP data channel
    Udp,
}

/// Create a channel pool with the given configuration
pub async fn create_pool<T: Transport + 'static>(
    config: PoolConfig,
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
) -> Result<Arc<TcpChannelPool<T>>> {
    TcpChannelPool::new(config, transport, remote_addr, session_key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type() {
        assert_eq!(ChannelType::Tcp, ChannelType::Tcp);
        assert_ne!(ChannelType::Tcp, ChannelType::Udp);
    }
}
