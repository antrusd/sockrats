//! Error types for Sockrats
//!
//! This module defines all custom error types used throughout the application.

use std::io;
use thiserror::Error;

/// Main error type for Sockrats operations
#[derive(Error, Debug)]
pub enum SockratsError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// SOCKS5 protocol error
    #[error("SOCKS5 error: {0}")]
    Socks5(#[from] Socks5Error),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Pool error
    #[error("Pool error: {0}")]
    Pool(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// SOCKS5 specific errors
#[derive(Error, Debug)]
pub enum Socks5Error {
    /// Unsupported SOCKS version
    #[error("Unsupported SOCKS version: {0}")]
    UnsupportedVersion(u8),

    /// No acceptable authentication method
    #[error("No acceptable authentication method")]
    NoAcceptableMethod,

    /// Authentication failed
    #[error("Authentication failed")]
    AuthFailed,

    /// Command not supported
    #[error("Command not supported: {0}")]
    CommandNotSupported(u8),

    /// Address type not supported
    #[error("Address type not supported: {0}")]
    AddressTypeNotSupported(u8),

    /// Connection refused
    #[error("Connection refused")]
    ConnectionRefused,

    /// Host unreachable
    #[error("Host unreachable")]
    HostUnreachable,

    /// Network unreachable
    #[error("Network unreachable")]
    NetworkUnreachable,

    /// General SOCKS server failure
    #[error("General SOCKS server failure")]
    GeneralFailure,

    /// Invalid address
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    /// Invalid domain name
    #[error("Invalid domain name: {0}")]
    InvalidDomain(String),
}

/// Reply codes for SOCKS5 protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Socks5ReplyCode {
    /// Command succeeded
    Succeeded = 0x00,
    /// General SOCKS server failure
    GeneralFailure = 0x01,
    /// Connection not allowed by ruleset
    ConnectionNotAllowed = 0x02,
    /// Network unreachable
    NetworkUnreachable = 0x03,
    /// Host unreachable
    HostUnreachable = 0x04,
    /// Connection refused
    ConnectionRefused = 0x05,
    /// TTL expired
    TtlExpired = 0x06,
    /// Command not supported
    CommandNotSupported = 0x07,
    /// Address type not supported
    AddressTypeNotSupported = 0x08,
}

impl From<Socks5ReplyCode> for u8 {
    fn from(code: Socks5ReplyCode) -> Self {
        code as u8
    }
}

impl TryFrom<u8> for Socks5ReplyCode {
    type Error = Socks5Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Socks5ReplyCode::Succeeded),
            0x01 => Ok(Socks5ReplyCode::GeneralFailure),
            0x02 => Ok(Socks5ReplyCode::ConnectionNotAllowed),
            0x03 => Ok(Socks5ReplyCode::NetworkUnreachable),
            0x04 => Ok(Socks5ReplyCode::HostUnreachable),
            0x05 => Ok(Socks5ReplyCode::ConnectionRefused),
            0x06 => Ok(Socks5ReplyCode::TtlExpired),
            0x07 => Ok(Socks5ReplyCode::CommandNotSupported),
            0x08 => Ok(Socks5ReplyCode::AddressTypeNotSupported),
            _ => Err(Socks5Error::GeneralFailure),
        }
    }
}

impl From<&io::Error> for Socks5ReplyCode {
    fn from(err: &io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::ConnectionRefused => Socks5ReplyCode::ConnectionRefused,
            io::ErrorKind::TimedOut => Socks5ReplyCode::HostUnreachable,
            io::ErrorKind::AddrNotAvailable => Socks5ReplyCode::HostUnreachable,
            _ => Socks5ReplyCode::GeneralFailure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks5_reply_code_from_u8_valid() {
        assert_eq!(
            Socks5ReplyCode::try_from(0x00).unwrap(),
            Socks5ReplyCode::Succeeded
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x01).unwrap(),
            Socks5ReplyCode::GeneralFailure
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x02).unwrap(),
            Socks5ReplyCode::ConnectionNotAllowed
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x03).unwrap(),
            Socks5ReplyCode::NetworkUnreachable
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x04).unwrap(),
            Socks5ReplyCode::HostUnreachable
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x05).unwrap(),
            Socks5ReplyCode::ConnectionRefused
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x06).unwrap(),
            Socks5ReplyCode::TtlExpired
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x07).unwrap(),
            Socks5ReplyCode::CommandNotSupported
        );
        assert_eq!(
            Socks5ReplyCode::try_from(0x08).unwrap(),
            Socks5ReplyCode::AddressTypeNotSupported
        );
    }

    #[test]
    fn test_socks5_reply_code_from_u8_invalid() {
        assert!(Socks5ReplyCode::try_from(0xFF).is_err());
        assert!(Socks5ReplyCode::try_from(0x09).is_err());
        assert!(Socks5ReplyCode::try_from(100).is_err());
    }

    #[test]
    fn test_socks5_reply_code_to_u8() {
        assert_eq!(u8::from(Socks5ReplyCode::Succeeded), 0x00);
        assert_eq!(u8::from(Socks5ReplyCode::GeneralFailure), 0x01);
        assert_eq!(u8::from(Socks5ReplyCode::ConnectionNotAllowed), 0x02);
        assert_eq!(u8::from(Socks5ReplyCode::NetworkUnreachable), 0x03);
        assert_eq!(u8::from(Socks5ReplyCode::HostUnreachable), 0x04);
        assert_eq!(u8::from(Socks5ReplyCode::ConnectionRefused), 0x05);
        assert_eq!(u8::from(Socks5ReplyCode::TtlExpired), 0x06);
        assert_eq!(u8::from(Socks5ReplyCode::CommandNotSupported), 0x07);
        assert_eq!(u8::from(Socks5ReplyCode::AddressTypeNotSupported), 0x08);
    }

    #[test]
    fn test_socks5_reply_code_from_io_error() {
        let err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(
            Socks5ReplyCode::from(&err),
            Socks5ReplyCode::ConnectionRefused
        );

        let err = io::Error::new(io::ErrorKind::TimedOut, "timeout");
        assert_eq!(
            Socks5ReplyCode::from(&err),
            Socks5ReplyCode::HostUnreachable
        );

        let err = io::Error::new(io::ErrorKind::AddrNotAvailable, "addr not available");
        assert_eq!(
            Socks5ReplyCode::from(&err),
            Socks5ReplyCode::HostUnreachable
        );

        let err = io::Error::new(io::ErrorKind::Other, "other");
        assert_eq!(Socks5ReplyCode::from(&err), Socks5ReplyCode::GeneralFailure);

        let err = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        assert_eq!(Socks5ReplyCode::from(&err), Socks5ReplyCode::GeneralFailure);
    }

    #[test]
    fn test_sockrats_error_display() {
        let err = SockratsError::Config("invalid config".to_string());
        assert_eq!(format!("{}", err), "Configuration error: invalid config");

        let err = SockratsError::Protocol("bad protocol".to_string());
        assert_eq!(format!("{}", err), "Protocol error: bad protocol");

        let err = SockratsError::Auth("auth failed".to_string());
        assert_eq!(format!("{}", err), "Authentication error: auth failed");

        let err = SockratsError::Connection("connection error".to_string());
        assert_eq!(format!("{}", err), "Connection error: connection error");

        let err = SockratsError::Transport("transport error".to_string());
        assert_eq!(format!("{}", err), "Transport error: transport error");

        let err = SockratsError::Pool("pool error".to_string());
        assert_eq!(format!("{}", err), "Pool error: pool error");

        let err = SockratsError::Timeout("timeout".to_string());
        assert_eq!(format!("{}", err), "Timeout: timeout");

        let err = SockratsError::Serialization("serialization error".to_string());
        assert_eq!(
            format!("{}", err),
            "Serialization error: serialization error"
        );
    }

    #[test]
    fn test_sockrats_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::Other, "io error");
        let err: SockratsError = io_err.into();
        assert!(matches!(err, SockratsError::Io(_)));
    }

    #[test]
    fn test_sockrats_error_from_socks5() {
        let socks5_err = Socks5Error::AuthFailed;
        let err: SockratsError = socks5_err.into();
        assert!(matches!(err, SockratsError::Socks5(_)));
    }

    #[test]
    fn test_socks5_error_display() {
        let err = Socks5Error::UnsupportedVersion(4);
        assert_eq!(format!("{}", err), "Unsupported SOCKS version: 4");

        let err = Socks5Error::NoAcceptableMethod;
        assert_eq!(format!("{}", err), "No acceptable authentication method");

        let err = Socks5Error::AuthFailed;
        assert_eq!(format!("{}", err), "Authentication failed");

        let err = Socks5Error::CommandNotSupported(0xFF);
        assert_eq!(format!("{}", err), "Command not supported: 255");

        let err = Socks5Error::AddressTypeNotSupported(0x99);
        assert_eq!(format!("{}", err), "Address type not supported: 153");

        let err = Socks5Error::ConnectionRefused;
        assert_eq!(format!("{}", err), "Connection refused");

        let err = Socks5Error::HostUnreachable;
        assert_eq!(format!("{}", err), "Host unreachable");

        let err = Socks5Error::NetworkUnreachable;
        assert_eq!(format!("{}", err), "Network unreachable");

        let err = Socks5Error::GeneralFailure;
        assert_eq!(format!("{}", err), "General SOCKS server failure");

        let err = Socks5Error::InvalidAddress("bad addr".to_string());
        assert_eq!(format!("{}", err), "Invalid address: bad addr");

        let err = Socks5Error::InvalidDomain("bad.domain".to_string());
        assert_eq!(format!("{}", err), "Invalid domain name: bad.domain");
    }

    #[test]
    fn test_socks5_reply_code_clone_copy() {
        let code = Socks5ReplyCode::Succeeded;
        let code2 = code;
        assert_eq!(code, code2);
    }

    #[test]
    fn test_socks5_reply_code_debug() {
        let code = Socks5ReplyCode::Succeeded;
        assert_eq!(format!("{:?}", code), "Succeeded");
    }
}
