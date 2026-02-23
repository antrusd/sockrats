//! Template for creating a new service type
//!
//! This module provides a documented skeleton for adding new services
//! to Sockrats. To create a new service:
//!
//! 1. Copy this directory to `src/services/your_service/`
//! 2. Rename `TemplateServiceHandler` to your service name
//! 3. Implement the [`ServiceHandler`](crate::services::ServiceHandler) trait
//! 4. Add a [`ServiceType`](crate::config::ServiceType) variant in `src/config/client.rs`
//! 5. Add a match arm in [`create_service_handler()`](crate::services::create_service_handler)
//!    in `src/services/mod.rs`
//! 6. Re-export your module from `src/services/mod.rs`
//! 7. Add tests!
//!
//! # Example Implementation
//!
//! ```rust,ignore
//! use crate::services::{ServiceHandler, StreamDyn};
//! use anyhow::Result;
//!
//! #[derive(Debug)]
//! pub struct MyServiceHandler {
//!     // Your service configuration
//! }
//!
//! #[async_trait::async_trait]
//! impl ServiceHandler for MyServiceHandler {
//!     fn service_type(&self) -> &str {
//!         "my_service"
//!     }
//!
//!     async fn handle_tcp_stream(&self, stream: Box<dyn StreamDyn>) -> Result<()> {
//!         // Implement your protocol handling here.
//!         // The stream is already connected and authenticated at the
//!         // rathole protocol level.
//!         todo!("Implement service protocol")
//!     }
//!
//!     // Optionally override:
//!     // - handle_udp_stream() for UDP support
//!     // - is_healthy() for health checks
//!     // - validate() for configuration validation
//! }
//! ```

#[cfg(test)]
mod tests {
    #[test]
    fn test_template_module_compiles() {
        // This test ensures the template module is included in the build
        // and serves as a reminder that any new service should have tests.
        assert!(true);
    }
}
