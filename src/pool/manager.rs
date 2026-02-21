//! Pool manager for health monitoring and maintenance
//!
//! Handles pool size maintenance and health checks.

use crate::config::PoolConfig;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tracing::debug;

/// Statistics for the connection pool
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Total channels created
    pub total_created: AtomicUsize,
    /// Channels currently in the pool
    pub pooled_count: AtomicUsize,
    /// Channels currently in use
    pub in_use_count: AtomicUsize,
    /// Total channels acquired
    pub total_acquired: AtomicUsize,
    /// Total channels returned
    pub total_returned: AtomicUsize,
    /// Total channels expired/removed
    pub total_expired: AtomicUsize,
}

impl PoolStats {
    /// Create new pool stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a channel creation
    pub fn record_created(&self) {
        self.total_created.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a channel acquisition
    pub fn record_acquired(&self) {
        self.total_acquired.fetch_add(1, Ordering::Relaxed);
        self.in_use_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a channel return
    pub fn record_returned(&self) {
        self.total_returned.fetch_add(1, Ordering::Relaxed);
        self.in_use_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a channel expiration
    pub fn record_expired(&self) {
        self.total_expired.fetch_add(1, Ordering::Relaxed);
    }

    /// Update pooled count
    pub fn set_pooled_count(&self, count: usize) {
        self.pooled_count.store(count, Ordering::Relaxed);
    }

    /// Get current stats snapshot
    pub fn snapshot(&self) -> PoolStatsSnapshot {
        PoolStatsSnapshot {
            total_created: self.total_created.load(Ordering::Relaxed),
            pooled_count: self.pooled_count.load(Ordering::Relaxed),
            in_use_count: self.in_use_count.load(Ordering::Relaxed),
            total_acquired: self.total_acquired.load(Ordering::Relaxed),
            total_returned: self.total_returned.load(Ordering::Relaxed),
            total_expired: self.total_expired.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of pool statistics
#[derive(Debug, Clone)]
pub struct PoolStatsSnapshot {
    pub total_created: usize,
    pub pooled_count: usize,
    pub in_use_count: usize,
    pub total_acquired: usize,
    pub total_returned: usize,
    pub total_expired: usize,
}

/// Pool manager for background maintenance
pub struct PoolManager {
    /// Configuration
    config: PoolConfig,
    /// Statistics
    stats: Arc<PoolStats>,
    /// Shutdown signal
    shutdown: Arc<Notify>,
    /// Whether shutdown has been requested
    is_shutdown: Arc<AtomicBool>,
}

impl PoolManager {
    /// Create a new pool manager
    pub fn new(config: PoolConfig, stats: Arc<PoolStats>) -> Self {
        PoolManager {
            config,
            stats,
            shutdown: Arc::new(Notify::new()),
            is_shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> &Arc<PoolStats> {
        &self.stats
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        self.is_shutdown.store(true, Ordering::SeqCst);
        self.shutdown.notify_one();
    }

    /// Check if shutdown has been requested
    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(Ordering::SeqCst)
    }

    /// Get the health check interval
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.config.health_check_interval)
    }

    /// Get the idle timeout
    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.config.idle_timeout)
    }

    /// Wait for shutdown signal
    pub async fn wait_shutdown(&self) {
        self.shutdown.notified().await;
    }

    /// Log pool health status
    pub fn log_health(&self) {
        let stats = self.stats.snapshot();
        debug!(
            "Pool health: created={}, pooled={}, in_use={}, acquired={}, returned={}, expired={}",
            stats.total_created,
            stats.pooled_count,
            stats.in_use_count,
            stats.total_acquired,
            stats.total_returned,
            stats.total_expired
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_new() {
        let stats = PoolStats::new();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_created, 0);
        assert_eq!(snapshot.pooled_count, 0);
        assert_eq!(snapshot.in_use_count, 0);
    }

    #[test]
    fn test_pool_stats_record_created() {
        let stats = PoolStats::new();
        stats.record_created();
        stats.record_created();

        assert_eq!(stats.snapshot().total_created, 2);
    }

    #[test]
    fn test_pool_stats_record_acquired_returned() {
        let stats = PoolStats::new();

        stats.record_acquired();
        assert_eq!(stats.snapshot().in_use_count, 1);
        assert_eq!(stats.snapshot().total_acquired, 1);

        stats.record_returned();
        assert_eq!(stats.snapshot().in_use_count, 0);
        assert_eq!(stats.snapshot().total_returned, 1);
    }

    #[test]
    fn test_pool_stats_record_expired() {
        let stats = PoolStats::new();
        stats.record_expired();
        stats.record_expired();
        stats.record_expired();

        assert_eq!(stats.snapshot().total_expired, 3);
    }

    #[test]
    fn test_pool_manager_new() {
        let config = PoolConfig::default();
        let stats = Arc::new(PoolStats::new());
        let manager = PoolManager::new(config.clone(), stats);

        assert_eq!(
            manager.health_check_interval(),
            Duration::from_secs(config.health_check_interval)
        );
        assert_eq!(
            manager.idle_timeout(),
            Duration::from_secs(config.idle_timeout)
        );
    }

    #[test]
    fn test_pool_manager_shutdown() {
        let config = PoolConfig::default();
        let stats = Arc::new(PoolStats::new());
        let manager = PoolManager::new(config, stats);

        assert!(!manager.is_shutdown());
        manager.shutdown();
        assert!(manager.is_shutdown());
    }
}
