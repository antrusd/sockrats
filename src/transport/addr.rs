//! Address handling with DNS caching
//!
//! Provides address resolution with optional caching to reduce DNS lookups.

use anyhow::{Context, Result};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Address that may have a cached resolved address
///
/// This type holds an address string and optionally caches the resolved
/// socket address to avoid repeated DNS lookups.
#[derive(Debug, Clone)]
pub struct AddrMaybeCached {
    /// The original address string
    addr: String,
    /// Cached resolved address
    cached: Arc<RwLock<Option<SocketAddr>>>,
}

impl AddrMaybeCached {
    /// Create a new address without cached resolution
    pub fn new(addr: &str) -> Self {
        AddrMaybeCached {
            addr: addr.to_string(),
            cached: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new address with a pre-resolved address
    pub fn with_cached(addr: &str, resolved: SocketAddr) -> Self {
        AddrMaybeCached {
            addr: addr.to_string(),
            cached: Arc::new(RwLock::new(Some(resolved))),
        }
    }

    /// Get the original address string
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Get the cached address if available
    pub async fn get_cached(&self) -> Option<SocketAddr> {
        *self.cached.read().await
    }

    /// Set the cached address
    pub async fn set_cached(&self, addr: SocketAddr) {
        *self.cached.write().await = Some(addr);
    }

    /// Clear the cached address
    pub async fn clear_cache(&self) {
        *self.cached.write().await = None;
    }

    /// Resolve the address, using cache if available
    ///
    /// If the address is already cached, returns the cached value.
    /// Otherwise, performs DNS resolution and caches the result.
    pub async fn resolve(&self) -> Result<SocketAddr> {
        // Check cache first
        if let Some(cached) = self.get_cached().await {
            return Ok(cached);
        }

        // Perform resolution
        let resolved = self.resolve_fresh().await?;

        // Cache the result
        self.set_cached(resolved).await;

        Ok(resolved)
    }

    /// Resolve the address without using cache
    pub async fn resolve_fresh(&self) -> Result<SocketAddr> {
        // Use blocking task for DNS resolution since ToSocketAddrs is blocking
        let addr = self.addr.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            addr.to_socket_addrs()
                .with_context(|| format!("Failed to resolve address: {}", addr))?
                .next()
                .with_context(|| format!("No addresses found for: {}", addr))
        })
        .await
        .with_context(|| "DNS resolution task panicked")??;

        Ok(resolved)
    }
}

impl From<SocketAddr> for AddrMaybeCached {
    fn from(addr: SocketAddr) -> Self {
        AddrMaybeCached {
            addr: addr.to_string(),
            cached: Arc::new(RwLock::new(Some(addr))),
        }
    }
}

impl From<&str> for AddrMaybeCached {
    fn from(addr: &str) -> Self {
        AddrMaybeCached::new(addr)
    }
}

impl From<String> for AddrMaybeCached {
    fn from(addr: String) -> Self {
        AddrMaybeCached::new(&addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_addr_maybe_cached_new() {
        let addr = AddrMaybeCached::new("example.com:80");
        assert_eq!(addr.addr(), "example.com:80");
        assert!(addr.get_cached().await.is_none());
    }

    #[tokio::test]
    async fn test_addr_maybe_cached_with_cached() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr = AddrMaybeCached::with_cached("localhost:8080", socket_addr);

        assert_eq!(addr.addr(), "localhost:8080");
        assert_eq!(addr.get_cached().await, Some(socket_addr));
    }

    #[tokio::test]
    async fn test_addr_maybe_cached_set_and_clear() {
        let addr = AddrMaybeCached::new("test:80");
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 80);

        // Initially empty
        assert!(addr.get_cached().await.is_none());

        // Set cache
        addr.set_cached(socket_addr).await;
        assert_eq!(addr.get_cached().await, Some(socket_addr));

        // Clear cache
        addr.clear_cache().await;
        assert!(addr.get_cached().await.is_none());
    }

    #[tokio::test]
    async fn test_addr_maybe_cached_from_socket_addr() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 443);
        let addr: AddrMaybeCached = socket_addr.into();

        assert_eq!(addr.get_cached().await, Some(socket_addr));
    }

    #[tokio::test]
    async fn test_addr_maybe_cached_resolve_localhost() {
        let addr = AddrMaybeCached::new("127.0.0.1:8080");
        let resolved = addr.resolve().await.unwrap();

        assert_eq!(resolved.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(resolved.port(), 8080);

        // Should be cached now
        assert!(addr.get_cached().await.is_some());
    }

    #[tokio::test]
    async fn test_addr_maybe_cached_uses_cache() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
        let addr = AddrMaybeCached::with_cached("invalid.invalid:1234", socket_addr);

        // Should return cached value even though the address is invalid
        let resolved = addr.resolve().await.unwrap();
        assert_eq!(resolved, socket_addr);
    }
}
