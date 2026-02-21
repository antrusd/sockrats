//! Pooled channel structure
//!
//! Represents a single channel in the pool with metadata.

use std::time::Instant;

/// A pooled channel with metadata
#[derive(Debug)]
pub struct PooledChannel<S> {
    /// The underlying stream
    pub(crate) stream: S,
    /// When the channel was created
    pub(crate) created_at: Instant,
    /// When the channel was last used
    pub(crate) last_used: Instant,
    /// Whether this is a TCP or UDP channel
    #[allow(dead_code)]
    pub(crate) is_tcp: bool,
}

impl<S> PooledChannel<S> {
    /// Create a new pooled TCP channel
    pub fn new_tcp(stream: S) -> Self {
        let now = Instant::now();
        PooledChannel {
            stream,
            created_at: now,
            last_used: now,
            is_tcp: true,
        }
    }

    /// Create a new pooled UDP channel
    pub fn new_udp(stream: S) -> Self {
        let now = Instant::now();
        PooledChannel {
            stream,
            created_at: now,
            last_used: now,
            is_tcp: false,
        }
    }

    /// Check if the channel is stale based on idle timeout
    pub fn is_stale(&self, idle_timeout: std::time::Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }

    /// Get the age of the channel
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    /// Get the time since last use
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_used.elapsed()
    }

    /// Mark the channel as used
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Get the underlying stream
    pub fn into_stream(self) -> S {
        self.stream
    }

    /// Get a reference to the stream
    pub fn stream(&self) -> &S {
        &self.stream
    }

    /// Get a mutable reference to the stream
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_pooled_channel_new_tcp() {
        let channel = PooledChannel::new_tcp(42);
        assert!(channel.is_tcp);
        assert!(channel.age() < Duration::from_secs(1));
        assert!(channel.idle_time() < Duration::from_secs(1));
    }

    #[test]
    fn test_pooled_channel_new_udp() {
        let channel = PooledChannel::new_udp("test");
        assert!(!channel.is_tcp);
    }

    #[test]
    fn test_pooled_channel_is_stale() {
        let channel = PooledChannel::new_tcp(0);

        // Should not be stale immediately
        assert!(!channel.is_stale(Duration::from_secs(1)));

        // With zero timeout, should be stale
        assert!(channel.is_stale(Duration::from_secs(0)));
    }

    #[test]
    fn test_pooled_channel_touch() {
        let mut channel = PooledChannel::new_tcp(0);
        let initial_last_used = channel.last_used;

        std::thread::sleep(Duration::from_millis(1));
        channel.touch();

        assert!(channel.last_used > initial_last_used);
    }

    #[test]
    fn test_pooled_channel_into_stream() {
        let channel = PooledChannel::new_tcp("my_stream");
        let stream = channel.into_stream();
        assert_eq!(stream, "my_stream");
    }

    #[test]
    fn test_pooled_channel_stream_ref() {
        let channel = PooledChannel::new_tcp(123);
        assert_eq!(*channel.stream(), 123);
    }

    #[test]
    fn test_pooled_channel_stream_mut() {
        let mut channel = PooledChannel::new_tcp(100);
        *channel.stream_mut() = 200;
        assert_eq!(*channel.stream(), 200);
    }
}
