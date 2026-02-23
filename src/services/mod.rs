//! Service module for Sockrats
//!
//! This module provides the extensible service architecture. All service types
//! (SOCKS5, SSH, etc.) live under this module and implement the [`ServiceHandler`]
//! trait for uniform handling by the client infrastructure.
//!
//! # Adding a New Service
//!
//! 1. Create a new directory under `src/services/your_service/`
//! 2. Implement the [`ServiceHandler`] trait
//! 3. Add a [`ServiceType`] variant in `src/config/client.rs`
//! 4. Add a match arm in [`create_service_handler()`]
//! 5. Re-export from this module
//!
//! See `src/services/template/mod.rs` for a documented skeleton.

pub mod socks;
pub mod ssh;
pub mod template;

use crate::config::{ServiceConfig, ServiceType, SocksConfig};
use anyhow::Result;
use ssh::SshConfig;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

// Re-export service handler implementations
pub use socks::Socks5ServiceHandler;
pub use ssh::SshServiceHandler;

/// Trait that all service handlers must implement.
///
/// A ServiceHandler processes incoming connections on data channels.
/// Each service type (SOCKS5, SSH, etc.) implements this trait to handle
/// its specific protocol over the tunnel stream.
///
/// # Required Methods
///
/// Only [`handle_tcp_stream`](ServiceHandler::handle_tcp_stream) is required.
/// All other methods have sensible defaults.
///
/// # Example
///
/// ```rust,ignore
/// use sockrats::services::ServiceHandler;
///
/// #[derive(Debug)]
/// struct MyHandler;
///
/// #[async_trait::async_trait]
/// impl ServiceHandler for MyHandler {
///     fn service_type(&self) -> &str { "my_service" }
///
///     async fn handle_tcp_stream(
///         &self,
///         stream: Box<dyn StreamDyn>,
///     ) -> anyhow::Result<()> {
///         // Handle the connection
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait ServiceHandler: Send + Sync + Debug {
    /// Human-readable name of this service type (e.g., "socks5", "ssh").
    fn service_type(&self) -> &str;

    /// Handle an incoming TCP stream from a data channel.
    ///
    /// The stream is already connected and authenticated at the rathole
    /// protocol level. The handler should implement the service-specific
    /// protocol (SOCKS5, SSH, etc.) on this stream.
    async fn handle_tcp_stream(&self, stream: Box<dyn StreamDyn>) -> Result<()>;

    /// Handle an incoming UDP data channel.
    ///
    /// Default implementation returns an error indicating UDP is not supported.
    /// Override this for services that support UDP (e.g., SOCKS5 UDP ASSOCIATE).
    async fn handle_udp_stream(&self, _stream: Box<dyn StreamDyn>) -> Result<()> {
        anyhow::bail!(
            "UDP not supported for service type: {}",
            self.service_type()
        )
    }

    /// Check if this service handler is healthy and ready to accept connections.
    ///
    /// Default implementation always returns `true`.
    fn is_healthy(&self) -> bool {
        true
    }

    /// Validate the handler's configuration.
    ///
    /// Called once during startup. Default implementation always succeeds.
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

/// A dynamic stream trait for service handlers.
///
/// This is a convenience alias combining the traits needed for service I/O.
/// It allows service handlers to accept any stream type (TCP, TLS, Noise, etc.)
/// without being generic over the transport.
pub trait StreamDyn: AsyncRead + AsyncWrite + Unpin + Send + Debug {}

/// Blanket implementation: any type implementing the required traits is a StreamDyn.
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Debug> StreamDyn for T {}

/// Registry that maps service names to their handlers.
///
/// Built during client startup from the configuration. The control channel
/// looks up the handler by service name when spawning data channels.
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    handlers: HashMap<String, Arc<dyn ServiceHandler>>,
}

impl ServiceRegistry {
    /// Create a new empty service registry.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a service name.
    pub fn register(&mut self, name: String, handler: Arc<dyn ServiceHandler>) {
        self.handlers.insert(name, handler);
    }

    /// Look up a handler by service name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ServiceHandler>> {
        self.handlers.get(name).cloned()
    }

    /// List all registered service names.
    pub fn service_names(&self) -> Vec<&str> {
        self.handlers.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered services.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

/// Create a [`ServiceHandler`] from a [`ServiceConfig`].
///
/// This factory function maps [`ServiceType`] variants to their concrete
/// handler implementations. When adding a new service type, add a match
/// arm here.
pub fn create_service_handler(service: &ServiceConfig) -> Result<Arc<dyn ServiceHandler>> {
    match service.service_type {
        ServiceType::Socks5 => {
            let config = service.socks.clone().unwrap_or_default();
            let handler = Socks5ServiceHandler::new(config);
            handler.validate()?;
            Ok(Arc::new(handler))
        }
        ServiceType::Ssh => {
            let config = service.ssh.clone().unwrap_or_default();
            let handler = SshServiceHandler::new(config);
            handler.validate()?;
            Ok(Arc::new(handler))
        }
    }
}

/// Create a [`ServiceHandler`] for legacy single-service mode.
///
/// In legacy mode, the service type is inferred from the service name:
/// names containing "ssh" create an SSH handler, everything else creates SOCKS5.
pub fn create_legacy_handler(
    service_name: &str,
    socks_config: &SocksConfig,
    ssh_config: &SshConfig,
) -> Arc<dyn ServiceHandler> {
    if service_name.to_lowercase().contains("ssh") {
        Arc::new(SshServiceHandler::new(ssh_config.clone()))
    } else {
        Arc::new(Socks5ServiceHandler::new(socks_config.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal mock service handler for testing
    #[derive(Debug)]
    struct MockServiceHandler {
        name: String,
    }

    #[async_trait::async_trait]
    impl ServiceHandler for MockServiceHandler {
        fn service_type(&self) -> &str {
            &self.name
        }

        async fn handle_tcp_stream(&self, _stream: Box<dyn StreamDyn>) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_service_registry_new() {
        let registry = ServiceRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_service_registry_register_and_get() {
        let mut registry = ServiceRegistry::new();
        let handler: Arc<dyn ServiceHandler> = Arc::new(MockServiceHandler {
            name: "test".to_string(),
        });

        registry.register("my-service".to_string(), handler);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let retrieved = registry.get("my-service");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().service_type(), "test");
    }

    #[test]
    fn test_service_registry_get_nonexistent() {
        let registry = ServiceRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_service_registry_service_names() {
        let mut registry = ServiceRegistry::new();
        registry.register(
            "socks5".to_string(),
            Arc::new(MockServiceHandler {
                name: "socks5".to_string(),
            }),
        );
        registry.register(
            "ssh".to_string(),
            Arc::new(MockServiceHandler {
                name: "ssh".to_string(),
            }),
        );

        let mut names = registry.service_names();
        names.sort();
        assert_eq!(names, vec!["socks5", "ssh"]);
    }

    #[test]
    fn test_service_registry_overwrite() {
        let mut registry = ServiceRegistry::new();
        registry.register(
            "svc".to_string(),
            Arc::new(MockServiceHandler {
                name: "v1".to_string(),
            }),
        );
        registry.register(
            "svc".to_string(),
            Arc::new(MockServiceHandler {
                name: "v2".to_string(),
            }),
        );

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("svc").unwrap().service_type(), "v2");
    }

    #[test]
    fn test_default_service_handler_methods() {
        let handler = MockServiceHandler {
            name: "test".to_string(),
        };
        assert!(handler.is_healthy());
        assert!(handler.validate().is_ok());
    }

    #[test]
    fn test_create_service_handler_socks5() {
        let service = ServiceConfig {
            name: "socks5".to_string(),
            service_type: ServiceType::Socks5,
            token: "token".to_string(),
            socks: Some(SocksConfig::default()),
            ssh: None,
        };

        let handler = create_service_handler(&service).unwrap();
        assert_eq!(handler.service_type(), "socks5");
    }

    #[test]
    fn test_create_service_handler_ssh() {
        let service = ServiceConfig {
            name: "ssh".to_string(),
            service_type: ServiceType::Ssh,
            token: "token".to_string(),
            socks: None,
            ssh: None,
        };

        let handler = create_service_handler(&service).unwrap();
        assert_eq!(handler.service_type(), "ssh");
    }

    #[test]
    fn test_create_legacy_handler_socks5() {
        let handler =
            create_legacy_handler("my-proxy", &SocksConfig::default(), &SshConfig::default());
        assert_eq!(handler.service_type(), "socks5");
    }

    #[test]
    fn test_create_legacy_handler_ssh() {
        let handler = create_legacy_handler(
            "my-ssh-tunnel",
            &SocksConfig::default(),
            &SshConfig::default(),
        );
        assert_eq!(handler.service_type(), "ssh");
    }

    #[test]
    fn test_create_legacy_handler_ssh_case_insensitive() {
        let handler = create_legacy_handler(
            "MySSHTunnel",
            &SocksConfig::default(),
            &SshConfig::default(),
        );
        assert_eq!(handler.service_type(), "ssh");
    }

    #[test]
    fn test_stream_dyn_blanket_impl() {
        // Verify that common stream types satisfy StreamDyn
        // This is a compile-time check; if it compiles, the blanket impl works
        fn assert_stream_dyn<T: StreamDyn>() {}
        assert_stream_dyn::<tokio::io::DuplexStream>();
    }
}
