//! TCP channel pool implementation
//!
//! Manages a pool of pre-established TCP data channels.

use super::channel::PooledChannel;
use super::guard::{PooledChannelGuard, ReturnedChannel};
use super::manager::{PoolManager, PoolStats};
use crate::config::PoolConfig;
use crate::protocol::{
    read_data_cmd, write_hello, DataChannelCmd, Digest, Hello, CURRENT_PROTO_VERSION,
};
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, Notify, Semaphore};
use tracing::{debug, info, warn};

/// TCP channel pool
pub struct TcpChannelPool<T: Transport> {
    /// Pool configuration
    config: PoolConfig,
    /// Transport for creating connections
    transport: Arc<T>,
    /// Remote server address
    remote_addr: AddrMaybeCached,
    /// Session key for data channel authentication
    session_key: Digest,
    /// Available channels
    channels: Mutex<VecDeque<PooledChannel<T::Stream>>>,
    /// Semaphore to limit concurrent channel creation
    create_semaphore: Semaphore,
    /// Notification when channels become available
    available_notify: Notify,
    /// Current number of active channels (pooled + in use)
    active_count: AtomicUsize,
    /// Pool manager
    manager: PoolManager,
    /// Channel for returning streams to the pool
    return_tx: mpsc::Sender<ReturnedChannel<T::Stream>>,
}

impl<T: Transport + 'static> TcpChannelPool<T> {
    /// Create a new TCP channel pool
    pub async fn new(
        config: PoolConfig,
        transport: Arc<T>,
        remote_addr: AddrMaybeCached,
        session_key: Digest,
    ) -> Result<Arc<Self>> {
        let stats = Arc::new(PoolStats::new());
        let manager = PoolManager::new(config.clone(), stats);

        let (return_tx, return_rx) = mpsc::channel(config.max_tcp_channels);

        let pool = Arc::new(TcpChannelPool {
            config: config.clone(),
            transport,
            remote_addr,
            session_key,
            channels: Mutex::new(VecDeque::new()),
            create_semaphore: Semaphore::new(config.max_tcp_channels),
            available_notify: Notify::new(),
            active_count: AtomicUsize::new(0),
            manager,
            return_tx,
        });

        // Start return handler
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            pool_clone.run_return_handler(return_rx).await;
        });

        // Warm up the pool
        pool.warm_up().await?;

        // Start maintenance task
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            pool_clone.run_maintenance().await;
        });

        Ok(pool)
    }

    /// Warm up the pool with minimum channels
    async fn warm_up(self: &Arc<Self>) -> Result<()> {
        info!(
            "Warming up TCP channel pool: {} channels",
            self.config.min_tcp_channels
        );

        let mut tasks = Vec::new();
        for _ in 0..self.config.min_tcp_channels {
            let pool = self.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = pool.create_channel().await {
                    warn!("Failed to pre-create channel: {:?}", e);
                }
            }));
        }

        for task in tasks {
            let _ = task.await;
        }

        info!("TCP channel pool warmed up");
        Ok(())
    }

    /// Create a new channel and add to pool
    async fn create_channel(&self) -> Result<()> {
        // Check if we're at capacity
        if self.active_count.load(Ordering::Relaxed) >= self.config.max_tcp_channels {
            return Ok(());
        }

        // Acquire semaphore permit
        let _permit = self
            .create_semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Semaphore closed"))?;

        // Double-check after acquiring permit
        if self.active_count.load(Ordering::Relaxed) >= self.config.max_tcp_channels {
            return Ok(());
        }

        // Establish the data channel
        let stream = self.establish_data_channel().await?;

        // Add to pool
        let mut channels = self.channels.lock().await;
        channels.push_back(PooledChannel::new_tcp(stream));
        self.active_count.fetch_add(1, Ordering::Relaxed);
        self.manager.stats().record_created();
        self.manager.stats().set_pooled_count(channels.len());

        self.available_notify.notify_one();
        debug!(
            "Created new TCP channel, pool size: {}, active: {}",
            channels.len(),
            self.active_count.load(Ordering::Relaxed)
        );

        Ok(())
    }

    /// Establish a data channel with the server
    async fn establish_data_channel(&self) -> Result<T::Stream> {
        let mut conn = self
            .transport
            .connect(&self.remote_addr)
            .await
            .context("Failed to connect to server")?;

        // Apply socket options
        T::hint(&conn, SocketOpts::for_data_channel());

        // Send data channel hello
        let hello = Hello::DataChannelHello(CURRENT_PROTO_VERSION, self.session_key);
        write_hello(&mut conn, &hello).await?;

        // Wait for command
        let cmd = read_data_cmd(&mut conn).await?;

        match cmd {
            DataChannelCmd::StartForwardTcp => Ok(conn),
            other => anyhow::bail!("Unexpected data channel command: {:?}", other),
        }
    }

    /// Acquire a channel from the pool
    pub async fn acquire(&self) -> Result<PooledChannelGuard<T::Stream>> {
        let timeout = Duration::from_secs(self.config.acquire_timeout);
        let deadline = Instant::now() + timeout;

        loop {
            // Try to get a channel from the pool
            {
                let mut channels = self.channels.lock().await;

                // Remove stale channels
                let idle_timeout = Duration::from_secs(self.config.idle_timeout);
                while let Some(front) = channels.front() {
                    if front.is_stale(idle_timeout) {
                        channels.pop_front();
                        self.active_count.fetch_sub(1, Ordering::Relaxed);
                        self.manager.stats().record_expired();
                        debug!("Removed stale TCP channel");
                    } else {
                        break;
                    }
                }

                if let Some(mut channel) = channels.pop_front() {
                    channel.touch();
                    self.manager.stats().set_pooled_count(channels.len());
                    self.manager.stats().record_acquired();

                    let is_tcp = channel.is_tcp();
                    return Ok(PooledChannelGuard::new(
                        channel.into_stream(),
                        self.return_tx.clone(),
                        is_tcp,
                    ));
                }
            }

            // No channel available, try to create one
            if self.active_count.load(Ordering::Relaxed) < self.config.max_tcp_channels {
                if let Err(e) = self.create_channel().await {
                    warn!("Failed to create channel on demand: {:?}", e);
                }
                continue;
            }

            // At capacity, wait for a channel
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("Timeout waiting for TCP channel");
            }

            tokio::select! {
                _ = self.available_notify.notified() => continue,
                _ = tokio::time::sleep(remaining) => {
                    anyhow::bail!("Timeout waiting for TCP channel");
                }
            }
        }
    }

    /// Run the return handler
    async fn run_return_handler(
        self: Arc<Self>,
        mut rx: mpsc::Receiver<ReturnedChannel<T::Stream>>,
    ) {
        while let Some(returned) = rx.recv().await {
            let mut channels = self.channels.lock().await;

            if channels.len() < self.config.max_tcp_channels {
                let channel = if returned.is_tcp {
                    PooledChannel::new_tcp(returned.stream)
                } else {
                    PooledChannel::new_udp(returned.stream)
                };
                channels.push_back(channel);
                self.manager.stats().record_returned();
                self.manager.stats().set_pooled_count(channels.len());
                self.available_notify.notify_one();
                debug!("Channel returned to pool, size: {}", channels.len());
            } else {
                // Pool is full, drop the channel
                self.active_count.fetch_sub(1, Ordering::Relaxed);
                debug!("Pool full, dropping returned channel");
            }
        }
    }

    /// Run maintenance tasks
    async fn run_maintenance(self: Arc<Self>) {
        let interval = self.manager.health_check_interval();

        loop {
            tokio::select! {
                _ = self.manager.wait_shutdown() => {
                    info!("Pool maintenance shutting down");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    self.maintain().await;
                }
            }
        }
    }

    /// Perform pool maintenance
    async fn maintain(&self) {
        // Ensure minimum channels
        let current = {
            let channels = self.channels.lock().await;
            channels.len()
        };

        if current < self.config.min_tcp_channels {
            let needed = self.config.min_tcp_channels - current;
            debug!("Replenishing pool: need {} channels", needed);

            for _ in 0..needed {
                if let Err(e) = self.create_channel().await {
                    warn!("Failed to replenish channel: {:?}", e);
                }
            }
        }

        self.manager.log_health();
    }

    /// Shutdown the pool
    pub fn shutdown(&self) {
        self.manager.shutdown();
    }

    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        self.manager.stats()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::PoolConfig;

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert!(config.min_tcp_channels <= config.max_tcp_channels);
        assert!(config.acquire_timeout > 0);
    }
}
