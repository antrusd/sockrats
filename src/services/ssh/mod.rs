//! SSH server service module
//!
//! This module provides an embedded SSH server that processes SSH connections
//! directly from tunnel streams without binding to a local port.
//!
//! # Architecture
//!
//! The SSH server uses the `russh` crate's `run_stream()` function to process
//! SSH protocol on any stream that implements `AsyncRead + AsyncWrite`. This
//! allows it to handle SSH connections from rathole data channels directly.
//!
//! # Example
//!
//! ```ignore
//! use sockrats::ssh::{handle_ssh_on_stream, SshConfig};
//! use std::sync::Arc;
//!
//! async fn example<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
//!     stream: S,
//!     config: Arc<SshConfig>,
//! ) -> Result<()> {
//!     handle_ssh_on_stream(stream, config).await
//! }
//! ```

pub mod auth;
pub mod config;
pub mod handler;
pub mod keys;
pub mod process;
pub mod session;

pub use config::SshConfig;
pub use handler::SshHandler;

use crate::services::{ServiceHandler, StreamDyn};

#[cfg(feature = "ssh")]
use anyhow::Result;
#[cfg(feature = "ssh")]
use auth::PublicKeyAuth;
#[cfg(feature = "ssh")]
use russh::server::Config as RusshConfig;
#[cfg(feature = "ssh")]
use russh::MethodKind;
#[cfg(feature = "ssh")]
use std::time::Duration;
#[cfg(feature = "ssh")]
use tokio::io::{AsyncRead, AsyncWrite};

use std::sync::Arc;

/// Handle an SSH connection on a stream
///
/// This function processes a complete SSH session on the given stream,
/// implementing the SSH server protocol using russh.
///
/// # Arguments
///
/// * `stream` - The stream to process SSH on (typically a tunnel data channel)
/// * `config` - SSH server configuration
///
/// # Example
///
/// ```ignore
/// let stream = /* tunnel data channel */;
/// let config = Arc::new(SshConfig::default());
/// handle_ssh_on_stream(stream, config).await?;
/// ```
#[cfg(feature = "ssh")]
pub async fn handle_ssh_on_stream<S>(stream: S, config: Arc<SshConfig>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    tracing::info!("Starting SSH session on stream");

    // Build russh config
    let russh_config = build_russh_config(&config)?;

    // Initialize public key authenticator if enabled
    let pubkey_auth = PublicKeyAuth::from_config(&config)?;

    // Create handler
    let handler = SshHandler::new(config.clone(), pubkey_auth);

    // Run SSH server on the stream
    let session = russh::server::run_stream(Arc::new(russh_config), stream, handler).await?;

    // Wait for session to complete
    session.await?;

    tracing::info!("SSH session completed");
    Ok(())
}

/// Handle an SSH connection with a timeout
#[cfg(feature = "ssh")]
pub async fn handle_ssh_with_timeout<S>(
    stream: S,
    config: Arc<SshConfig>,
    timeout: Duration,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    tokio::time::timeout(timeout, handle_ssh_on_stream(stream, config))
        .await
        .map_err(|_| anyhow::anyhow!("SSH session timed out"))?
}

/// Build russh server configuration from our SshConfig
#[cfg(feature = "ssh")]
fn build_russh_config(config: &SshConfig) -> Result<RusshConfig> {
    use russh::MethodSet;
    use russh::SshId;

    // Load host key
    let host_key = match &config.host_key {
        Some(path) => keys::load_host_key(path)?,
        None => {
            tracing::warn!("No host key configured, generating temporary Ed25519 key");
            keys::generate_ed25519_key()?
        }
    };

    // Build authentication methods
    let mut methods = MethodSet::empty();
    if config.has_password_auth() {
        methods.push(MethodKind::Password);
    }
    if config.has_publickey_auth() {
        methods.push(MethodKind::PublicKey);
    }

    Ok(RusshConfig {
        server_id: SshId::Standard(config.server_id.clone()),
        methods,
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_secs(0)),
        keys: vec![host_key],
        max_auth_attempts: config.max_auth_tries as usize,
        inactivity_timeout: Some(Duration::from_secs(config.connection_timeout)),
        ..Default::default()
    })
}

/// Placeholder for when SSH feature is disabled
#[cfg(not(feature = "ssh"))]
pub async fn handle_ssh_on_stream<S>(
    _stream: S,
    _config: std::sync::Arc<SshConfig>,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    anyhow::bail!("SSH feature is not enabled. Recompile with --features ssh")
}

/// SSH service handler implementing the [`ServiceHandler`] trait.
///
/// Wraps the existing SSH server implementation to conform to the
/// service handler interface, allowing it to be registered in the
/// [`ServiceRegistry`](crate::services::ServiceRegistry).
#[derive(Debug)]
pub struct SshServiceHandler {
    config: Arc<SshConfig>,
}

impl SshServiceHandler {
    /// Create a new SSH service handler with the given configuration.
    pub fn new(config: SshConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Get a reference to the SSH configuration.
    pub fn config(&self) -> &SshConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl ServiceHandler for SshServiceHandler {
    fn service_type(&self) -> &str {
        "ssh"
    }

    async fn handle_tcp_stream(&self, stream: Box<dyn StreamDyn>) -> Result<()> {
        handle_ssh_on_stream(stream, self.config.clone()).await
    }

    fn validate(&self) -> Result<()> {
        if self.config.enabled {
            self.config.validate().map_err(|e| anyhow::anyhow!(e))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all expected items are exported
        let _config = SshConfig::default();
    }

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert!(!config.enabled);
        assert!(config.has_password_auth());
        assert!(config.has_publickey_auth());
    }

    #[test]
    fn test_ssh_service_handler_new() {
        let handler = SshServiceHandler::new(SshConfig::default());
        assert_eq!(handler.service_type(), "ssh");
    }

    #[test]
    fn test_ssh_service_handler_config() {
        let config = SshConfig {
            enabled: true,
            ..Default::default()
        };
        let handler = SshServiceHandler::new(config);
        assert!(handler.config().enabled);
    }

    #[test]
    fn test_ssh_service_handler_is_healthy() {
        let handler = SshServiceHandler::new(SshConfig::default());
        assert!(handler.is_healthy());
    }

    #[test]
    fn test_ssh_service_handler_validate_disabled() {
        let handler = SshServiceHandler::new(SshConfig::default());
        assert!(handler.validate().is_ok());
    }

    #[test]
    fn test_ssh_service_handler_debug() {
        let handler = SshServiceHandler::new(SshConfig::default());
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("SshServiceHandler"));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_build_russh_config_default() {
        let config = SshConfig::default();
        let russh_config = build_russh_config(&config).unwrap();

        // Should generate a temporary key
        assert!(!russh_config.keys.is_empty());
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_build_russh_config_password_only() {
        let config = SshConfig {
            enabled: true,
            auth_methods: vec!["password".to_string()],
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };
        let russh_config = build_russh_config(&config).unwrap();

        // Should have methods configured
        assert!(!russh_config.methods.is_empty());
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_build_russh_config_inactivity_timeout() {
        let config = SshConfig {
            connection_timeout: 600,
            ..Default::default()
        };
        let russh_config = build_russh_config(&config).unwrap();

        assert_eq!(
            russh_config.inactivity_timeout,
            Some(Duration::from_secs(600))
        );
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_build_russh_config_server_id() {
        let config = SshConfig {
            server_id: "SSH-2.0-TestServer".to_string(),
            ..Default::default()
        };
        let russh_config = build_russh_config(&config).unwrap();

        match russh_config.server_id {
            russh::SshId::Standard(id) => assert_eq!(id, "SSH-2.0-TestServer"),
            russh::SshId::Raw(_) => panic!("Expected Standard SshId, got Raw"),
        }
    }

    #[tokio::test]
    #[cfg(not(feature = "ssh"))]
    async fn test_handle_ssh_disabled() {
        let stream = tokio_test::io::Builder::new().build();
        let config = std::sync::Arc::new(SshConfig::default());

        let result = handle_ssh_on_stream(stream, config).await;
        assert!(result.is_err());
    }
}
