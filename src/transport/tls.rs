//! TLS transport implementation
//!
//! Provides TLS-encrypted connections using rustls (pure Rust, easy static linking).
//!
//! This module requires either the `rustls-tls` or `native-tls` feature to be enabled.
//! The current implementation uses rustls for static linking compatibility.

use super::{AddrMaybeCached, SocketOpts, StreamDyn, Transport, TransportDyn};
use crate::config::{TlsConfig, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

/// TLS stream type alias
pub type TlsStream = tokio_rustls::client::TlsStream<TcpStream>;

/// TLS transport for encrypted connections using rustls
#[derive(Clone)]
pub struct TlsTransport {
    /// TLS connector
    connector: TlsConnector,
    /// Server hostname for verification
    hostname: Option<String>,
    /// Socket options to apply to connections
    socket_opts: SocketOpts,
    /// Connection timeout
    connect_timeout: Duration,
}

impl std::fmt::Debug for TlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsTransport")
            .field("hostname", &self.hostname)
            .field("socket_opts", &self.socket_opts)
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}

impl TlsTransport {
    /// Create a new TLS transport with the given configuration
    pub fn with_config(config: &TlsConfig, socket_opts: SocketOpts) -> Result<Self> {
        let mut root_store = RootCertStore::empty();

        // Add system root certificates
        let native_certs = rustls_native_certs::load_native_certs();
        for cert in native_certs.certs {
            root_store.add(cert).ok();
        }

        // Add custom trusted root if specified
        if let Some(ref root_path) = config.trusted_root {
            let file = std::fs::File::open(root_path)
                .with_context(|| format!("Failed to open certificate file: {}", root_path))?;
            let mut reader = BufReader::new(file);
            let certs = rustls_pemfile::certs(&mut reader)
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("Failed to parse certificates from: {}", root_path))?;
            for cert in certs {
                root_store
                    .add(cert)
                    .with_context(|| "Failed to add certificate to store")?;
            }
        }

        let mut tls_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // Configure certificate verification
        if config.skip_verify {
            // Create a config that doesn't verify certificates
            // This is dangerous and should only be used for testing
            tls_config = ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier))
                .with_no_client_auth();
        }

        Ok(TlsTransport {
            connector: TlsConnector::from(Arc::new(tls_config)),
            hostname: config.hostname.clone(),
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

/// Certificate verifier that accepts all certificates (dangerous!)
#[derive(Debug)]
struct NoVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        vec![
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA256,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA384,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA512,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA256,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA384,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA512,
            tokio_rustls::rustls::SignatureScheme::ED25519,
        ]
    }
}

#[async_trait]
impl Transport for TlsTransport {
    type Stream = TlsStream;

    fn new(config: &TransportConfig) -> Result<Self> {
        let tls_config = config
            .tls
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TLS configuration required for TLS transport"))?;

        let socket_opts = SocketOpts::from_tcp_config(&config.tcp);
        TlsTransport::with_config(tls_config, socket_opts)
    }

    fn hint(_conn: &Self::Stream, _opts: SocketOpts) {
        // Cannot apply TCP options to TLS stream directly
        // Options should be applied to underlying TCP stream before TLS handshake
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        let resolved = addr.resolve().await?;

        // Connect TCP first
        let tcp_stream = tokio::time::timeout(self.connect_timeout, TcpStream::connect(resolved))
            .await
            .with_context(|| format!("Connection timeout to {}", addr.addr()))?
            .with_context(|| format!("Failed to connect to {}", addr.addr()))?;

        // Apply socket options before TLS handshake
        self.socket_opts.apply(&tcp_stream)?;

        // Determine hostname for TLS verification
        let hostname = self.hostname.as_deref().unwrap_or_else(|| {
            // Extract hostname from address string
            addr.addr().split(':').next().unwrap_or("localhost")
        });

        let server_name =
            tokio_rustls::rustls::pki_types::ServerName::try_from(hostname.to_string())
                .with_context(|| format!("Invalid hostname: {}", hostname))?;

        // Perform TLS handshake
        let tls_stream = self
            .connector
            .connect(server_name, tcp_stream)
            .await
            .with_context(|| format!("TLS handshake failed with {}", hostname))?;

        tracing::debug!("TLS connection established to {} ({})", hostname, resolved);

        Ok(tls_stream)
    }
}

#[async_trait]
impl TransportDyn for TlsTransport {
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>> {
        let stream = self.connect(addr).await?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_transport_with_config() {
        let config = TlsConfig {
            hostname: Some("example.com".to_string()),
            trusted_root: None,
            skip_verify: true,
        };
        let socket_opts = SocketOpts::default();

        let transport = TlsTransport::with_config(&config, socket_opts);
        assert!(transport.is_ok());

        let transport = transport.unwrap();
        assert_eq!(transport.hostname, Some("example.com".to_string()));
    }

    #[test]
    fn test_tls_transport_with_connect_timeout() {
        let config = TlsConfig {
            hostname: Some("test.com".to_string()),
            trusted_root: None,
            skip_verify: true,
        };
        let socket_opts = SocketOpts::default();

        let transport = TlsTransport::with_config(&config, socket_opts)
            .unwrap()
            .with_connect_timeout(Duration::from_secs(30));

        assert_eq!(transport.connect_timeout, Duration::from_secs(30));
    }
}
