//! RAII guard for pooled channels
//!
//! Provides automatic return of channels to the pool when dropped.

use std::ops::{Deref, DerefMut};
use tokio::sync::mpsc;

/// RAII guard that returns the channel to the pool on drop
///
/// This guard holds a channel and ensures it is returned to the pool
/// when it goes out of scope, unless explicitly taken.
pub struct PooledChannelGuard<S: Send + 'static> {
    /// The stream (Option to allow taking)
    stream: Option<S>,
    /// Channel to return the stream to the pool
    return_tx: Option<mpsc::Sender<ReturnedChannel<S>>>,
    /// Whether this is a TCP channel
    is_tcp: bool,
}

/// A channel being returned to the pool
pub struct ReturnedChannel<S> {
    /// The stream
    pub stream: S,
    /// Whether this is a TCP channel
    pub is_tcp: bool,
}

impl<S: Send + 'static> PooledChannelGuard<S> {
    /// Create a new guard
    pub fn new(stream: S, return_tx: mpsc::Sender<ReturnedChannel<S>>, is_tcp: bool) -> Self {
        PooledChannelGuard {
            stream: Some(stream),
            return_tx: Some(return_tx),
            is_tcp,
        }
    }

    /// Take ownership of the stream (won't return to pool)
    pub fn take(mut self) -> S {
        self.return_tx = None; // Disable return
        self.stream.take().expect("Stream already taken")
    }

    /// Check if this is a TCP channel
    pub fn is_tcp(&self) -> bool {
        self.is_tcp
    }

    /// Get a reference to the stream
    pub fn stream(&self) -> &S {
        self.stream.as_ref().expect("Stream already taken")
    }

    /// Get a mutable reference to the stream
    pub fn stream_mut(&mut self) -> &mut S {
        self.stream.as_mut().expect("Stream already taken")
    }
}

impl<S: Send + 'static> Deref for PooledChannelGuard<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        self.stream.as_ref().expect("Stream already taken")
    }
}

impl<S: Send + 'static> DerefMut for PooledChannelGuard<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.stream.as_mut().expect("Stream already taken")
    }
}

impl<S: Send + 'static> Drop for PooledChannelGuard<S> {
    fn drop(&mut self) {
        if let (Some(stream), Some(return_tx)) = (self.stream.take(), self.return_tx.take()) {
            let returned = ReturnedChannel {
                stream,
                is_tcp: self.is_tcp,
            };
            // Try to return to pool, ignore if channel is closed
            let _ = return_tx.try_send(returned);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_guard_take() {
        let (tx, mut rx) = mpsc::channel(1);
        let guard = PooledChannelGuard::new(42, tx, true);

        let value = guard.take();
        assert_eq!(value, 42);

        // Should not receive anything since we took the stream
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_guard_drop_returns_to_pool() {
        let (tx, mut rx) = mpsc::channel(1);

        {
            let _guard = PooledChannelGuard::new(123, tx, false);
            // Guard dropped here
        }

        // Should receive the returned channel
        let returned = rx.try_recv().unwrap();
        assert_eq!(returned.stream, 123);
        assert!(!returned.is_tcp);
    }

    #[test]
    fn test_guard_deref() {
        let (tx, _rx) = mpsc::channel(1);
        let guard = PooledChannelGuard::new(String::from("test"), tx, true);

        // Deref
        assert_eq!(&*guard, "test");
        assert_eq!(guard.len(), 4); // String method through deref
    }

    #[test]
    fn test_guard_deref_mut() {
        let (tx, _rx) = mpsc::channel(1);
        let mut guard = PooledChannelGuard::new(vec![1, 2, 3], tx, true);

        // DerefMut
        guard.push(4);
        assert_eq!(&*guard, &vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_guard_is_tcp() {
        let (tx, _rx) = mpsc::channel::<ReturnedChannel<i32>>(1);
        let guard = PooledChannelGuard::new(0, tx.clone(), true);
        assert!(guard.is_tcp());

        let guard = PooledChannelGuard::new(0, tx, false);
        assert!(!guard.is_tcp());
    }
}
