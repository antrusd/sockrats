//! Connection pool configuration
//!
//! Defines configuration for the data channel connection pool.

use serde::{Deserialize, Serialize};

/// Default minimum TCP channels
fn default_min_tcp_channels() -> usize {
    2
}

/// Default maximum TCP channels
fn default_max_tcp_channels() -> usize {
    10
}

/// Default minimum UDP channels
fn default_min_udp_channels() -> usize {
    1
}

/// Default maximum UDP channels
fn default_max_udp_channels() -> usize {
    5
}

/// Default idle timeout in seconds
fn default_idle_timeout() -> u64 {
    300
}

/// Default health check interval in seconds
fn default_health_check_interval() -> u64 {
    30
}

/// Default acquire timeout in seconds
fn default_acquire_timeout() -> u64 {
    10
}

/// Connection pool configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PoolConfig {
    /// Minimum number of pre-established TCP channels
    #[serde(default = "default_min_tcp_channels")]
    pub min_tcp_channels: usize,

    /// Maximum number of TCP channels
    #[serde(default = "default_max_tcp_channels")]
    pub max_tcp_channels: usize,

    /// Minimum number of pre-established UDP channels
    #[serde(default = "default_min_udp_channels")]
    pub min_udp_channels: usize,

    /// Maximum number of UDP channels
    #[serde(default = "default_max_udp_channels")]
    pub max_udp_channels: usize,

    /// Channel idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,

    /// Health check interval in seconds
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,

    /// Maximum time to wait for a channel from the pool
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            min_tcp_channels: default_min_tcp_channels(),
            max_tcp_channels: default_max_tcp_channels(),
            min_udp_channels: default_min_udp_channels(),
            max_udp_channels: default_max_udp_channels(),
            idle_timeout: default_idle_timeout(),
            health_check_interval: default_health_check_interval(),
            acquire_timeout: default_acquire_timeout(),
        }
    }
}

impl PoolConfig {
    /// Validate the pool configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.min_tcp_channels > self.max_tcp_channels {
            return Err("min_tcp_channels cannot be greater than max_tcp_channels".to_string());
        }
        if self.min_udp_channels > self.max_udp_channels {
            return Err("min_udp_channels cannot be greater than max_udp_channels".to_string());
        }
        if self.max_tcp_channels == 0 {
            return Err("max_tcp_channels must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_tcp_channels, 2);
        assert_eq!(config.max_tcp_channels, 10);
        assert_eq!(config.min_udp_channels, 1);
        assert_eq!(config.max_udp_channels, 5);
        assert_eq!(config.idle_timeout, 300);
        assert_eq!(config.health_check_interval, 30);
        assert_eq!(config.acquire_timeout, 10);
    }

    #[test]
    fn test_pool_config_validate_valid() {
        let config = PoolConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pool_config_validate_invalid_tcp() {
        let config = PoolConfig {
            min_tcp_channels: 20,
            max_tcp_channels: 10,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("tcp_channels"));
    }

    #[test]
    fn test_pool_config_validate_invalid_udp() {
        let config = PoolConfig {
            min_udp_channels: 10,
            max_udp_channels: 5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("udp_channels"));
    }

    #[test]
    fn test_pool_config_validate_zero_max() {
        let config = PoolConfig {
            max_tcp_channels: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}
