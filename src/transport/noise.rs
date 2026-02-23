//! Noise protocol transport implementation
//!
//! Provides encrypted connections using the Noise protocol framework.

use super::{AddrMaybeCached, SocketOpts, StreamDyn, Transport, TransportDyn};
use crate::config::{NoiseConfig, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use snowstorm::NoiseStream;
use std::time::Duration;
use tokio::net::TcpStream;

/// Noise transport for encrypted connections using Noise protocol
#[derive(Debug)]
pub struct NoiseTransport {
    /// Noise pattern (e.g., "Noise_NK_25519_ChaChaPoly_BLAKE2s")
    pattern: String,
    /// Local private key (optional for some patterns)
    local_private_key: Option<Vec<u8>>,
    /// Remote public key
    remote_public_key: Vec<u8>,
    /// Socket options to apply to connections
    socket_opts: SocketOpts,
    /// Connection timeout
    connect_timeout: Duration,
}

impl NoiseTransport {
    /// Create a new Noise transport with the given configuration
    pub fn with_config(config: &NoiseConfig, socket_opts: SocketOpts) -> Result<Self> {
        // Decode the remote public key from base64
        let remote_public_key = BASE64
            .decode(&config.remote_public_key)
            .with_context(|| "Failed to decode remote public key from base64")?;

        // Decode local private key if provided
        let local_private_key = if let Some(ref key) = config.local_private_key {
            Some(
                BASE64
                    .decode(key)
                    .with_context(|| "Failed to decode local private key from base64")?,
            )
        } else {
            None
        };

        Ok(NoiseTransport {
            pattern: config.pattern.clone(),
            local_private_key,
            remote_public_key,
            socket_opts,
            connect_timeout: Duration::from_secs(10),
        })
    }

    /// Set connection timeout
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
}

#[async_trait]
impl Transport for NoiseTransport {
    type Stream = NoiseStream<TcpStream>;

    fn new(config: &TransportConfig) -> Result<Self> {
        let noise_config = config
            .noise
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Noise configuration required for Noise transport"))?;

        let socket_opts = SocketOpts::from_tcp_config(&config.tcp);
        NoiseTransport::with_config(noise_config, socket_opts)
    }

    fn hint(_conn: &Self::Stream, _opts: SocketOpts) {
        // Cannot apply TCP options to Noise stream directly
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        let resolved = addr.resolve().await?;

        // Connect TCP first
        let tcp_stream = tokio::time::timeout(self.connect_timeout, TcpStream::connect(resolved))
            .await
            .with_context(|| format!("Connection timeout to {}", addr.addr()))?
            .with_context(|| format!("Failed to connect to {}", addr.addr()))?;

        // Apply socket options before Noise handshake
        self.socket_opts.apply(&tcp_stream)?;

        // Build Noise initiator using snowstorm
        let mut builder = snowstorm::Builder::new(self.pattern.parse()?)
            .remote_public_key(&self.remote_public_key);

        if let Some(ref key) = self.local_private_key {
            builder = builder.local_private_key(key);
        }

        // Build initiator and perform handshake
        let handshake_state = builder.build_initiator()?;

        // Perform Noise handshake using snowstorm's NoiseStream
        let noise_stream = NoiseStream::handshake(tcp_stream, handshake_state)
            .await
            .with_context(|| "Noise handshake failed")?;

        tracing::debug!("Noise connection established to {}", resolved);

        Ok(noise_stream)
    }
}

#[async_trait]
impl TransportDyn for NoiseTransport {
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>> {
        let stream = self.connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_keypair() -> (String, String) {
        // Generate a test keypair for testing
        // In real usage, keys should be generated securely
        use snowstorm::Builder;

        let builder = Builder::new("Noise_NK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
        let keypair = builder.generate_keypair().unwrap();

        let private_key = BASE64.encode(&keypair.private);
        let public_key = BASE64.encode(&keypair.public);

        (private_key, public_key)
    }

    #[test]
    fn test_noise_transport_with_config() {
        let (_, public_key) = create_test_keypair();

        let config = NoiseConfig {
            pattern: "Noise_NK_25519_ChaChaPoly_BLAKE2s".to_string(),
            local_private_key: None,
            remote_public_key: public_key,
        };
        let socket_opts = SocketOpts::default();

        let transport = NoiseTransport::with_config(&config, socket_opts);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_noise_transport_with_invalid_key() {
        let config = NoiseConfig {
            pattern: "Noise_NK_25519_ChaChaPoly_BLAKE2s".to_string(),
            local_private_key: None,
            remote_public_key: "not-valid-base64!!!".to_string(),
        };
        let socket_opts = SocketOpts::default();

        let transport = NoiseTransport::with_config(&config, socket_opts);
        assert!(transport.is_err());
    }

    #[test]
    fn test_noise_transport_with_connect_timeout() {
        let (_, public_key) = create_test_keypair();

        let config = NoiseConfig {
            pattern: "Noise_NK_25519_ChaChaPoly_BLAKE2s".to_string(),
            local_private_key: None,
            remote_public_key: public_key,
        };

        let transport = NoiseTransport::with_config(&config, SocketOpts::default())
            .unwrap()
            .with_connect_timeout(Duration::from_secs(30));

        assert_eq!(transport.connect_timeout, Duration::from_secs(30));
    }
}
