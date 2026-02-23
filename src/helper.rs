//! Helper utilities for Sockrats
//!
//! This module provides common utility functions used throughout the application.

use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

/// Default buffer size for IO operations
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default connection timeout in seconds
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default heartbeat timeout in seconds
pub const DEFAULT_HEARTBEAT_TIMEOUT_SECS: u64 = 40;

/// Bidirectional copy between two async streams
///
/// Copies data from `a` to `b` and from `b` to `a` concurrently.
/// Returns when either direction encounters an error or EOF.
pub async fn copy_bidirectional<A, B>(a: &mut A, b: &mut B) -> std::io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin + ?Sized,
    B: AsyncRead + AsyncWrite + Unpin + ?Sized,
{
    tokio::io::copy_bidirectional(a, b).await
}

/// Parse duration from seconds
pub fn duration_from_secs(secs: u64) -> Duration {
    Duration::from_secs(secs)
}

/// Retry configuration for operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration
    pub fn new(max_retries: u32) -> Self {
        RetryConfig {
            max_retries,
            ..Default::default()
        }
    }

    /// Calculate delay for a given attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return self.initial_delay;
        }

        let delay_ms = self.initial_delay.as_millis() as f64 * self.multiplier.powi(attempt as i32);
        let delay = Duration::from_millis(delay_ms as u64);

        std::cmp::min(delay, self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_BUFFER_SIZE, 8192);
        assert_eq!(DEFAULT_CONNECT_TIMEOUT_SECS, 10);
        assert_eq!(DEFAULT_HEARTBEAT_TIMEOUT_SECS, 40);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert_eq!(config.multiplier, 2.0);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(5);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
    }

    #[test]
    fn test_retry_config_new_with_zero() {
        let config = RetryConfig::new(0);
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_retry_config_delay_for_attempt() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
            multiplier: 2.0,
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
        // Should be capped at max_delay
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(10), Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_delay_exponential_backoff() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(60),
            multiplier: 3.0,
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(10));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(30));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(90));
    }

    #[test]
    fn test_retry_config_clone() {
        let config = RetryConfig::new(5);
        let config2 = config.clone();
        assert_eq!(config.max_retries, config2.max_retries);
        assert_eq!(config.initial_delay, config2.initial_delay);
    }

    #[test]
    fn test_retry_config_debug() {
        let config = RetryConfig::new(5);
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RetryConfig"));
        assert!(debug_str.contains("max_retries"));
    }

    #[test]
    fn test_duration_from_secs() {
        assert_eq!(duration_from_secs(5), Duration::from_secs(5));
        assert_eq!(duration_from_secs(0), Duration::from_secs(0));
        assert_eq!(duration_from_secs(60), Duration::from_secs(60));
        assert_eq!(duration_from_secs(3600), Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn test_copy_bidirectional() {
        let (mut a1, mut a2) = duplex(1024);
        let (mut b1, mut b2) = duplex(1024);

        // Spawn copy task
        let copy_task = tokio::spawn(async move { copy_bidirectional(&mut a2, &mut b2).await });

        // Send data from a1 to b1 through the bidirectional copy
        a1.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        b1.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        // Send data from b1 to a1 through the bidirectional copy
        b1.write_all(b"world").await.unwrap();
        let mut buf = [0u8; 5];
        a1.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");

        // Close streams to finish the copy task
        drop(a1);
        drop(b1);

        // Wait for copy to complete
        let result = tokio::time::timeout(Duration::from_millis(100), copy_task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_copy_bidirectional_large_data() {
        let (mut a1, mut a2) = duplex(65536);
        let (mut b1, mut b2) = duplex(65536);

        let copy_task = tokio::spawn(async move { copy_bidirectional(&mut a2, &mut b2).await });

        // Send large data
        let large_data = vec![0xAB; 50000];
        a1.write_all(&large_data).await.unwrap();

        let mut received = vec![0u8; 50000];
        b1.read_exact(&mut received).await.unwrap();
        assert_eq!(received, large_data);

        drop(a1);
        drop(b1);

        let _ = tokio::time::timeout(Duration::from_millis(100), copy_task).await;
    }
}
