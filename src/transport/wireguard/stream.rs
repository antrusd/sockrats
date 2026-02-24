//! Async stream over the WireGuard tunnel.
//!
//! [`WireguardStream`] implements `AsyncRead + AsyncWrite + Unpin + Send +
//! Sync + Debug` so it can be used as a drop-in replacement for
//! `TcpStream` or `NoiseStream` in the sockrats transport layer.
//!
//! Each stream corresponds to one virtual TCP connection inside the
//! smoltcp stack, communicated via mpsc channels with the event loop.

use bytes::{Bytes, BytesMut};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

/// Channel capacity for stream ↔ event loop communication.
const STREAM_CHANNEL_CAPACITY: usize = 256;

/// Messages sent from a [`WireguardStream`] to the event loop.
#[derive(Debug)]
pub enum StreamMessage {
    /// Application data to send through the virtual TCP socket.
    Data(Bytes),
    /// Request to flush pending data.
    Flush,
    /// Request to close the virtual TCP socket gracefully.
    Close,
}

/// Channels connecting a [`WireguardStream`] to the event loop.
///
/// The event loop holds the other ends of these channels.
pub struct StreamChannelPair {
    /// Send inbound data TO the WireguardStream.
    pub inbound_tx: mpsc::Sender<Bytes>,
    /// Receive outbound data FROM the WireguardStream.
    pub outbound_rx: mpsc::Receiver<StreamMessage>,
}

/// A virtual TCP stream over the WireGuard tunnel.
///
/// Created by the event loop when [`WireguardTransport::connect()`]
/// is called.  Communicates with the event loop via channels:
///
/// - **Outbound**: application writes → `StreamMessage::Data` → event loop
///   → smoltcp TCP send buffer → IP packets → boringtun → UDP.
/// - **Inbound**: UDP → boringtun → IP packets → smoltcp TCP recv buffer →
///   event loop → `Bytes` → application reads.
pub struct WireguardStream {
    /// Channel to send outbound data/commands to the event loop.
    outbound_tx: mpsc::Sender<StreamMessage>,
    /// Channel to receive inbound data from the event loop.
    inbound_rx: mpsc::Receiver<Bytes>,
    /// Buffer for partially consumed incoming data.
    read_buf: BytesMut,
    /// Unique stream identifier (maps to smoltcp SocketHandle index).
    stream_id: u32,
    /// Whether shutdown has been initiated.
    closed: Arc<AtomicBool>,
}

impl WireguardStream {
    /// Create a new stream and the corresponding channel pair for the
    /// event loop.
    ///
    /// Returns `(stream, channel_pair)`.  The event loop should hold
    /// the `channel_pair` and forward data between the smoltcp socket
    /// and the channels.
    pub fn new_pair(stream_id: u32) -> (Self, StreamChannelPair) {
        let (inbound_tx, inbound_rx) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let (outbound_tx, outbound_rx) = mpsc::channel(STREAM_CHANNEL_CAPACITY);

        let stream = WireguardStream {
            outbound_tx,
            inbound_rx,
            read_buf: BytesMut::new(),
            stream_id,
            closed: Arc::new(AtomicBool::new(false)),
        };

        let channels = StreamChannelPair {
            inbound_tx,
            outbound_rx,
        };

        (stream, channels)
    }

    /// Get the stream identifier.
    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Check if the stream has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

impl AsyncRead for WireguardStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // 1. Drain any leftover data in read_buf first.
        if !self.read_buf.is_empty() {
            let to_copy = std::cmp::min(self.read_buf.len(), buf.remaining());
            buf.put_slice(&self.read_buf.split_to(to_copy));
            return Poll::Ready(Ok(()));
        }

        // 2. Poll the inbound channel for new data.
        match self.inbound_rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                if data.is_empty() {
                    // Empty data signals EOF from the event loop.
                    return Poll::Ready(Ok(()));
                }
                let to_copy = std::cmp::min(data.len(), buf.remaining());
                buf.put_slice(&data[..to_copy]);
                // Store any remainder in the read buffer.
                if to_copy < data.len() {
                    self.read_buf.extend_from_slice(&data[to_copy..]);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                // Channel closed — treat as EOF.
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for WireguardStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.closed.load(Ordering::Relaxed) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "WireguardStream is closed",
            )));
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let msg = StreamMessage::Data(Bytes::copy_from_slice(buf));

        match self.outbound_tx.try_send(msg) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel full — register the waker so we get retried.
                // We use reserve() to get a Permit which polls properly.
                let tx = self.outbound_tx.clone();
                let data = Bytes::copy_from_slice(buf);
                // Create a future for the send and poll it
                let mut send_fut =
                    Box::pin(async move { tx.send(StreamMessage::Data(data)).await });
                match send_fut.as_mut().poll(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(buf.len())),
                    Poll::Ready(Err(_)) => Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Event loop channel closed",
                    ))),
                    Poll::Pending => Poll::Pending,
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Event loop channel closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Send a flush hint — best effort (don't block).
        let _ = self.outbound_tx.try_send(StreamMessage::Flush);
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.closed.store(true, Ordering::Relaxed);
        let _ = self.outbound_tx.try_send(StreamMessage::Close);
        Poll::Ready(Ok(()))
    }
}

// We need Send + Sync for the Transport trait bound.
// mpsc::Sender is Send + Sync, mpsc::Receiver is Send but not Sync.
// However since WireguardStream is only accessed by one task at a time
// (through &mut self in AsyncRead/AsyncWrite), this is safe.
unsafe impl Sync for WireguardStream {}

impl std::fmt::Debug for WireguardStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WireguardStream")
            .field("stream_id", &self.stream_id)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .field("read_buf_len", &self.read_buf.len())
            .finish()
    }
}

impl Drop for WireguardStream {
    fn drop(&mut self) {
        // Signal the event loop to close the virtual TCP socket if we
        // haven't already.
        if !self.closed.swap(true, Ordering::Relaxed) {
            let _ = self.outbound_tx.try_send(StreamMessage::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_new_pair() {
        let (stream, _channels) = WireguardStream::new_pair(42);
        assert_eq!(stream.stream_id(), 42);
        assert!(!stream.is_closed());
    }

    #[tokio::test]
    async fn test_read_from_channel() {
        let (mut stream, channels) = WireguardStream::new_pair(1);

        // Send data through the inbound channel
        channels
            .inbound_tx
            .send(Bytes::from_static(b"hello"))
            .await
            .unwrap();

        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[tokio::test]
    async fn test_read_partial() {
        let (mut stream, channels) = WireguardStream::new_pair(1);

        // Send more data than we'll read at once
        channels
            .inbound_tx
            .send(Bytes::from_static(b"hello world"))
            .await
            .unwrap();

        // Read only 5 bytes
        let mut buf = [0u8; 5];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");

        // Read remaining 6 bytes from the internal buffer
        let mut buf2 = [0u8; 10];
        let n2 = stream.read(&mut buf2).await.unwrap();
        assert_eq!(n2, 6);
        assert_eq!(&buf2[..6], b" world");
    }

    #[tokio::test]
    async fn test_write_to_channel() {
        let (mut stream, mut channels) = WireguardStream::new_pair(1);

        stream.write_all(b"test data").await.unwrap();

        // Read from the outbound channel
        match channels.outbound_rx.recv().await.unwrap() {
            StreamMessage::Data(data) => {
                assert_eq!(&data[..], b"test data");
            }
            other => panic!("Expected Data, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_flush_sends_message() {
        let (mut stream, mut channels) = WireguardStream::new_pair(1);

        stream.flush().await.unwrap();

        match channels.outbound_rx.recv().await.unwrap() {
            StreamMessage::Flush => {}
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_shutdown_sends_close() {
        let (mut stream, mut channels) = WireguardStream::new_pair(1);

        stream.shutdown().await.unwrap();
        assert!(stream.is_closed());

        match channels.outbound_rx.recv().await.unwrap() {
            StreamMessage::Close => {}
            other => panic!("Expected Close, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_eof_on_channel_close() {
        let (mut stream, channels) = WireguardStream::new_pair(1);

        // Drop the sender to close the channel
        drop(channels.inbound_tx);

        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 0); // EOF
    }

    #[tokio::test]
    async fn test_write_after_close_errors() {
        let (mut stream, _channels) = WireguardStream::new_pair(1);

        stream.shutdown().await.unwrap();

        let result = stream.write(b"data").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_drop_sends_close() {
        let (stream, mut channels) = WireguardStream::new_pair(1);

        drop(stream);

        // Should receive a Close message
        match channels.outbound_rx.recv().await {
            Some(StreamMessage::Close) => {}
            other => panic!("Expected Close on drop, got {:?}", other),
        }
    }

    #[test]
    fn test_debug_impl() {
        let (stream, _channels) = WireguardStream::new_pair(99);
        let debug = format!("{:?}", stream);
        assert!(debug.contains("WireguardStream"));
        assert!(debug.contains("99"));
    }
}
