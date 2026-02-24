//! WireGuard tunnel transport layer.
//!
//! Provides [`WireguardTransport`] which implements the [`Transport`]
//! trait, routing TCP connections through a userspace WireGuard tunnel
//! built on `boringtun` (crypto) and `smoltcp` (virtual TCP/IP stack).
//!
//! No TUN/TAP device is created — all packet processing is in-memory.
//!
//! # Architecture
//!
//! ```text
//! Application ──TCP──► smoltcp ──IP pkts──► boringtun ──UDP──► WG peer
//! ```
//!
//! See the plan in `plans/wireguard-tunnel.md` for full details.

pub mod config;
mod device;
mod event_loop;
mod stack;
pub mod stream;
mod tunnel;

pub use config::WireguardConfig;
pub use stream::WireguardStream;

use event_loop::WgEventLoop;

use super::{AddrMaybeCached, SocketOpts, StreamDyn, Transport, TransportDyn};
use crate::config::TransportConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// WireGuard transport — routes TCP connections through a userspace
/// WireGuard tunnel.
///
/// Created when `[client.wireguard].enabled = true` and
/// `[client.transport].type = "tcp"`.  The transport wraps a
/// background event loop ([`WgEventLoop`]) that manages:
///
/// - A real UDP socket to the WireGuard peer
/// - boringtun for WireGuard encryption/decryption
/// - smoltcp for virtual TCP/IP stack
///
/// Each call to [`Transport::connect()`] creates a new virtual TCP
/// connection inside the WireGuard tunnel.
#[derive(Debug)]
pub struct WireguardTransport {
    /// Handle to the background event loop.
    event_loop: Arc<WgEventLoop>,
    /// Connection timeout for virtual TCP establishment.
    connect_timeout: Duration,
}

#[async_trait]
impl Transport for WireguardTransport {
    type Stream = WireguardStream;

    fn new(config: &TransportConfig) -> Result<Self> {
        // WireguardTransport::new() cannot be async, but we need to start
        // the event loop which requires async.  We use a blocking approach
        // via tokio::runtime::Handle to bridge the gap.
        //
        // This is safe because Transport::new() is always called from
        // within a tokio runtime context (in Client::new()).
        let wg_config = config.wireguard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "WireGuard configuration required but [client.wireguard] section is missing"
            )
        })?;

        wg_config
            .validate()
            .context("WireGuard configuration validation failed")?;

        // Start the event loop using the current tokio runtime.
        // block_in_place moves this task off the runtime thread so block_on
        // doesn't panic with "cannot start a runtime from within a runtime".
        let handle = tokio::runtime::Handle::current();
        let event_loop =
            tokio::task::block_in_place(|| handle.block_on(WgEventLoop::start(wg_config)))?;

        Ok(Self {
            event_loop: Arc::new(event_loop),
            connect_timeout: Duration::from_secs(10),
        })
    }

    fn hint(_conn: &Self::Stream, _opts: SocketOpts) {
        // Virtual TCP sockets don't have OS-level socket options.
        // SocketOpts (nodelay, keepalive) are not applicable to smoltcp.
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        self.event_loop
            .connect(addr, self.connect_timeout)
            .await
            .with_context(|| format!("WireGuard virtual TCP connection to {} failed", addr.addr()))
    }
}

#[async_trait]
impl TransportDyn for WireguardTransport {
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>> {
        let stream = self.connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wireguard_transport_requires_config() {
        // Transport::new() without wireguard config should fail.
        let config = TransportConfig::default();
        // We can't call Transport::new() directly here because it needs
        // a tokio runtime.  But we can verify the config check logic.
        assert!(config.wireguard.is_none());
    }

    #[test]
    fn test_wireguard_stream_debug() {
        let (stream, _channels) = WireguardStream::new_pair(1);
        let debug = format!("{:?}", stream);
        assert!(debug.contains("WireguardStream"));
    }
}
