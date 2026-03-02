//! Error types for the VNC server service.

use std::io;
use thiserror::Error;

/// Result type for VNC operations.
pub type Result<T> = std::result::Result<T, VncError>;

/// Errors that can occur in VNC server operations.
#[derive(Debug, Error)]
pub enum VncError {
    /// I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// VNC protocol error.
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Authentication failed.
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Invalid pixel format.
    #[error("Invalid pixel format")]
    InvalidPixelFormat,

    /// Encoding error.
    #[error("Encoding error: {0}")]
    Encoding(String),

    /// Invalid operation or state.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Connection closed.
    #[error("Connection closed")]
    ConnectionClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vnc_error_io() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "reset");
        let vnc_err: VncError = io_err.into();
        assert!(matches!(vnc_err, VncError::Io(_)));
        assert!(vnc_err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_vnc_error_protocol() {
        let err = VncError::Protocol("invalid version".to_string());
        assert_eq!(err.to_string(), "Protocol error: invalid version");
    }

    #[test]
    fn test_vnc_error_auth_failed() {
        let err = VncError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");
    }

    #[test]
    fn test_vnc_error_invalid_pixel_format() {
        let err = VncError::InvalidPixelFormat;
        assert_eq!(err.to_string(), "Invalid pixel format");
    }

    #[test]
    fn test_vnc_error_encoding() {
        let err = VncError::Encoding("zlib failed".to_string());
        assert_eq!(err.to_string(), "Encoding error: zlib failed");
    }

    #[test]
    fn test_vnc_error_invalid_operation() {
        let err = VncError::InvalidOperation("not ready".to_string());
        assert_eq!(err.to_string(), "Invalid operation: not ready");
    }

    #[test]
    fn test_vnc_error_connection_closed() {
        let err = VncError::ConnectionClosed;
        assert_eq!(err.to_string(), "Connection closed");
    }

    #[test]
    fn test_result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: Result<i32> = Err(VncError::ConnectionClosed);
        assert!(err.is_err());
    }
}
