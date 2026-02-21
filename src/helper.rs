//! Helper utilities for SocksRat
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

        let delay_ms = self.initial_delay.as_millis() as f64
            * self.multiplier.powi(attempt as i32);
        let delay = Duration::from_millis(delay_ms as u64);

        std::cmp::min(delay, self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_duration_from_secs() {
        assert_eq!(duration_from_secs(5), Duration::from_secs(5));
        assert_eq!(duration_from_secs(0), Duration::from_secs(0));
    }
}
