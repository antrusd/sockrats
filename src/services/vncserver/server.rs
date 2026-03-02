//! VNC server shared state management.
//!
//! This module provides the [`VncServer`] struct which manages the shared
//! framebuffer and processes individual VNC client sessions. Unlike the
//! reference implementation which manages a TCP listener and multiple
//! concurrent clients, our version processes one session per stream
//! (as each rathole tunnel data channel is a separate stream).
//!
//! When the `xcap` feature is enabled, the server automatically starts
//! screen capture on the first client connection, mirroring the primary
//! monitor into the framebuffer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use super::capture::ScreenCapture;
use super::client::{ClientEvent, VncClient};
use super::config::VncConfig;
use super::framebuffer::Framebuffer;

/// Shared VNC server state.
///
/// Manages the framebuffer, screen capture, and configuration for VNC sessions.
/// Each incoming stream (from a rathole tunnel data channel) is
/// handled as an independent VNC session.
///
/// Screen capture is started lazily on the first client connection when the
/// `xcap` feature is enabled.
#[derive(Debug)]
pub struct VncServer {
    /// The shared framebuffer.
    framebuffer: Framebuffer,
    /// Desktop name sent to clients during ServerInit.
    desktop_name: String,
    /// Optional VNC password for authentication.
    password: Option<String>,
    /// Server configuration.
    config: Arc<VncConfig>,
    /// Screen capture controller (started lazily on first client connection).
    capture: tokio::sync::Mutex<Option<ScreenCapture>>,
    /// Whether capture has been attempted (avoids repeated attempts on failure).
    capture_attempted: AtomicBool,
}

impl VncServer {
    /// Creates a new [`VncServer`] with the given configuration.
    ///
    /// Initializes a framebuffer with the dimensions specified in the config.
    /// Screen capture is not started until the first client connects.
    pub fn new(config: VncConfig) -> Self {
        let framebuffer = Framebuffer::new(config.width, config.height);
        let desktop_name = config.desktop_name.clone();
        let password = config.password.clone();
        let max_fps = config.max_fps;

        // Create screen capture controller (not started yet)
        let capture = ScreenCapture::new(framebuffer.clone(), max_fps);

        let config = Arc::new(config);

        Self {
            framebuffer,
            desktop_name,
            password,
            config,
            capture: tokio::sync::Mutex::new(Some(capture)),
            capture_attempted: AtomicBool::new(false),
        }
    }

    /// Returns a reference to the shared framebuffer.
    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    /// Returns a reference to the server configuration.
    pub fn config(&self) -> &VncConfig {
        &self.config
    }

    /// Ensures screen capture is started (called lazily on first client connection).
    ///
    /// If capture has already been attempted (success or failure), this is a no-op.
    /// On failure, logs a warning and continues — the VNC server works without
    /// capture (headless framebuffer mode).
    async fn ensure_capture_started(&self) {
        if self.capture_attempted.load(Ordering::Relaxed) {
            return;
        }

        let mut guard = self.capture.lock().await;
        // Double-check after acquiring lock
        if self.capture_attempted.load(Ordering::Relaxed) {
            return;
        }

        if let Some(capture) = guard.as_mut() {
            match capture.start().await {
                Ok(monitor_info) => {
                    info!(
                        "Screen capture started: monitor '{}' ({}x{}, primary={})",
                        monitor_info.name,
                        monitor_info.width,
                        monitor_info.height,
                        monitor_info.is_primary
                    );
                }
                Err(e) => {
                    warn!(
                        "Screen capture unavailable (headless mode): {}. \
                         VNC server will serve a blank framebuffer.",
                        e
                    );
                }
            }
        }
        self.capture_attempted.store(true, Ordering::Release);
    }

    /// Stops screen capture if running.
    pub async fn stop_capture(&self) {
        let mut guard = self.capture.lock().await;
        if let Some(capture) = guard.as_mut() {
            capture.stop().await;
        }
    }

    /// Returns whether screen capture is currently running.
    pub async fn is_capture_running(&self) -> bool {
        let guard = self.capture.lock().await;
        guard.as_ref().is_some_and(|c| c.is_running())
    }

    /// Handle a single VNC session on the given stream.
    ///
    /// On the first call, starts screen capture to mirror the primary monitor
    /// into the framebuffer. Performs the VNC handshake, then enters the
    /// message loop until the client disconnects or an error occurs.
    ///
    /// # Arguments
    ///
    /// * `stream` - Any async stream implementing `AsyncRead + AsyncWrite + Unpin + Send`
    ///
    /// # Errors
    ///
    /// Returns an error if the handshake fails or an I/O error occurs.
    pub async fn handle_stream<S>(&self, stream: S) -> anyhow::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        // Start screen capture on first client connection
        self.ensure_capture_started().await;

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Create VNC client (performs handshake)
        let mut client = VncClient::new(
            stream,
            self.framebuffer.clone(),
            self.desktop_name.clone(),
            self.password.clone(),
            event_tx,
        )
        .await
        .map_err(|e| anyhow::anyhow!("VNC handshake failed: {}", e))?;

        // Register the client's dirty region receiver with the framebuffer
        let receiver = client.dirty_region_receiver();
        self.framebuffer.register_receiver(receiver).await;

        info!("VNC client connected");

        // Spawn event handler task
        let event_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    ClientEvent::KeyPress { down, key } => {
                        tracing::debug!("VNC key event: down={} key=0x{:X}", down, key);
                    }
                    ClientEvent::PointerMove { x, y, button_mask } => {
                        tracing::trace!(
                            "VNC pointer: ({}, {}) buttons=0x{:02X}",
                            x,
                            y,
                            button_mask
                        );
                    }
                    ClientEvent::CutText { text } => {
                        tracing::debug!("VNC cut text: {} bytes", text.len());
                    }
                    ClientEvent::Disconnected => {
                        info!("VNC client disconnected");
                        break;
                    }
                }
            }
        });

        // Run the message loop
        let result = client.handle_messages().await;

        // Clean up
        event_handle.abort();

        match result {
            Ok(()) => {
                info!("VNC session ended normally");
                Ok(())
            }
            Err(e) => {
                error!("VNC session error: {}", e);
                Err(anyhow::anyhow!("VNC session error: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vnc_server_new_defaults() {
        let config = VncConfig::default();
        let server = VncServer::new(config);

        assert_eq!(server.framebuffer().width(), 1024);
        assert_eq!(server.framebuffer().height(), 768);
        assert_eq!(server.desktop_name, "Sockrats VNC");
        assert!(server.password.is_none());
    }

    #[test]
    fn test_vnc_server_new_custom() {
        let config = VncConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            password: Some("secret".to_string()),
            desktop_name: "My Desktop".to_string(),
            ..VncConfig::default()
        };
        let server = VncServer::new(config);

        assert_eq!(server.framebuffer().width(), 1920);
        assert_eq!(server.framebuffer().height(), 1080);
        assert_eq!(server.desktop_name, "My Desktop");
        assert_eq!(server.password, Some("secret".to_string()));
    }

    #[test]
    fn test_vnc_server_config_ref() {
        let config = VncConfig {
            jpeg_quality: 90,
            compression_level: 3,
            ..VncConfig::default()
        };
        let server = VncServer::new(config);

        assert_eq!(server.config().jpeg_quality, 90);
        assert_eq!(server.config().compression_level, 3);
    }

    #[test]
    fn test_vnc_server_debug() {
        let server = VncServer::new(VncConfig::default());
        let debug = format!("{:?}", server);
        assert!(debug.contains("VncServer"));
    }

    #[test]
    fn test_vnc_server_framebuffer_accessible() {
        let server = VncServer::new(VncConfig {
            width: 640,
            height: 480,
            ..VncConfig::default()
        });

        let fb = server.framebuffer();
        assert_eq!(fb.width(), 640);
        assert_eq!(fb.height(), 480);
    }

    #[tokio::test]
    async fn test_vnc_server_handle_stream_invalid() {
        let server = VncServer::new(VncConfig::default());

        // A closed stream should fail handshake
        let (client, _server_end) = tokio::io::duplex(1);
        drop(_server_end); // Close immediately

        let result = server.handle_stream(client).await;
        assert!(result.is_err());
    }
}
