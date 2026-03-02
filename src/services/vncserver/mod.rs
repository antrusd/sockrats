//! VNC server service module for Sockrats.
//!
//! This module provides a VNC (Virtual Network Computing) server that runs
//! over rathole tunnel streams. It implements the RFB (Remote Framebuffer)
//! protocol per RFC 6143, with pure Rust dependencies (no C libraries).
//!
//! # Architecture
//!
//! ```text
//! rathole tunnel → VncServiceHandler → VncServer → VncClient
//!                                           │
//!                                      Framebuffer
//! ```
//!
//! # Feature Gate
//!
//! This module is only available when the `vncserver` feature is enabled.
//!
//! # Encodings
//!
//! Supported RFB encodings (via `rfb-encodings` crate):
//! - Raw, CopyRect, RRE, CoRRE, Hextile
//! - Zlib, ZlibHex, ZRLE
//! - Tight (with pure Rust JPEG via `jpeg-encoder`)
//!
//! # Example
//!
//! ```rust,ignore
//! use sockrats::services::vncserver::{VncServiceHandler, VncConfig};
//!
//! let config = VncConfig {
//!     enabled: true,
//!     width: 1920,
//!     height: 1080,
//!     password: Some("secret".to_string()),
//!     ..VncConfig::default()
//! };
//!
//! let handler = VncServiceHandler::new(config);
//! ```

mod auth;
mod capture;
mod client;
mod encoding;
mod framebuffer;
mod protocol;
mod server;

pub mod config;
pub mod error;

pub use config::VncConfig;
pub use error::{Result as VncResult, VncError};
pub use server::VncServer;

use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;

use super::{ServiceHandler, StreamDyn};

/// VNC service handler implementing the [`ServiceHandler`] trait.
///
/// This handler processes incoming tunnel streams as VNC client connections.
/// It maintains a shared [`VncServer`] instance with a framebuffer that
/// persists across connections.
#[derive(Debug)]
pub struct VncServiceHandler {
    /// Shared VNC server state (framebuffer, config).
    server: Arc<VncServer>,
}

impl VncServiceHandler {
    /// Creates a new [`VncServiceHandler`] with the given configuration.
    pub fn new(config: VncConfig) -> Self {
        Self {
            server: Arc::new(VncServer::new(config)),
        }
    }

    /// Returns a reference to the underlying VNC server.
    pub fn server(&self) -> &VncServer {
        &self.server
    }

    /// Returns the VNC configuration.
    pub fn config(&self) -> &VncConfig {
        self.server.config()
    }
}

impl Clone for VncServiceHandler {
    fn clone(&self) -> Self {
        Self {
            server: Arc::clone(&self.server),
        }
    }
}

#[async_trait::async_trait]
impl ServiceHandler for VncServiceHandler {
    fn service_type(&self) -> &str {
        "vncserver"
    }

    async fn handle_tcp_stream(&self, stream: Box<dyn StreamDyn>) -> Result<()> {
        self.server.handle_stream(stream).await
    }

    fn is_healthy(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        self.server
            .config()
            .validate()
            .map_err(|e| anyhow::anyhow!(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vnc_service_handler_new() {
        let handler = VncServiceHandler::new(VncConfig::default());
        assert_eq!(handler.service_type(), "vncserver");
    }

    #[test]
    fn test_vnc_service_handler_config() {
        let config = VncConfig {
            enabled: true,
            width: 800,
            height: 600,
            ..VncConfig::default()
        };
        let handler = VncServiceHandler::new(config);
        assert_eq!(handler.config().width, 800);
        assert_eq!(handler.config().height, 600);
    }

    #[test]
    fn test_vnc_service_handler_is_healthy() {
        let handler = VncServiceHandler::new(VncConfig::default());
        assert!(handler.is_healthy());
    }

    #[test]
    fn test_vnc_service_handler_validate() {
        let handler = VncServiceHandler::new(VncConfig::default());
        assert!(handler.validate().is_ok());
    }

    #[test]
    fn test_vnc_service_handler_validate_disabled() {
        let config = VncConfig {
            enabled: false,
            ..VncConfig::default()
        };
        let handler = VncServiceHandler::new(config);
        // validate() should succeed even when disabled
        assert!(handler.validate().is_ok());
    }

    #[test]
    fn test_vnc_service_handler_debug() {
        let handler = VncServiceHandler::new(VncConfig::default());
        let debug = format!("{:?}", handler);
        assert!(debug.contains("VncServiceHandler"));
    }

    #[test]
    fn test_vnc_service_handler_clone() {
        let handler = VncServiceHandler::new(VncConfig::default());
        let cloned = handler.clone();
        assert_eq!(cloned.service_type(), "vncserver");
        // Both should point to the same server (Arc)
        assert_eq!(handler.config().width, cloned.config().width);
    }

    #[test]
    fn test_vnc_service_handler_server_ref() {
        let handler = VncServiceHandler::new(VncConfig {
            width: 1920,
            height: 1080,
            ..VncConfig::default()
        });
        let server = handler.server();
        assert_eq!(server.framebuffer().width(), 1920);
        assert_eq!(server.framebuffer().height(), 1080);
    }
}
