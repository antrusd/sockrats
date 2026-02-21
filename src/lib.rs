//! # Sockrats - Reverse SOCKS5 Tunneling Client
//!
//! Sockrats is a Rust-based application that functions as a reverse tunneling
//! client with an embedded SOCKS5 server. It connects to a remote rathole server
//! and exposes a SOCKS5 proxy through that tunnel, without binding to any local
//! network interface.
//!
//! ## Features
//!
//! - **Client-Only Mode**: No server-side logic; connects to a standard rathole server
//! - **Reverse SOCKS Tunneling**: SOCKS5 traffic flows through the rathole tunnel
//! - **No Local Listeners**: SOCKS5 server operates purely in-memory on tunnel streams
//! - **Full UDP ASSOCIATE Support**: Complete UDP relay for DNS and other UDP protocols
//! - **Connection Pooling**: Pre-established data channel pool for improved performance
//!
//! ## Usage
//!
//! ```rust,ignore
//! use sockrats::config::load_config;
//! use sockrats::client::run_client;
//! use tokio::sync::broadcast;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = load_config("config.toml")?;
//!     let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
//!
//!     run_client(config, shutdown_rx).await
//! }
//! ```
//!
//! ## Architecture
//!
//! The client connects to a rathole server and waits for CreateDataChannel
//! commands. When a remote SOCKS5 client connects to the rathole server,
//! the server sends a CreateDataChannel command, and Sockrats establishes
//! a data channel to handle the SOCKS5 request.
//!
//! ```text
//! SOCKS5 Client -> Rathole Server -> Sockrats -> Target
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod client;
pub mod config;
pub mod error;
pub mod helper;
pub mod pool;
pub mod protocol;
pub mod socks;
pub mod ssh;
pub mod transport;

// Re-export commonly used items
pub use client::run_client;
pub use config::{load_config, Config};
pub use error::{Socks5Error, SockratsError};

/// Version of the Sockrats library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Name of the application
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_name() {
        assert_eq!(NAME, "sockrats");
    }
}
