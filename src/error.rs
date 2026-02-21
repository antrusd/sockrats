//! Error types for SocksRat
//!
//! This module defines all custom error types used throughout the application.

use std::io;
use thiserror::Error;

/// Main error type for SocksRat operations
#[derive(Error, Debug)]
pub enum SocksRatError {
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
            Socks5ReplyCode::try_from(0x05).unwrap(),
            Socks5ReplyCode::ConnectionRefused
        );
    }

    #[test]
    fn test_socks5_reply_code_to_u8() {
        assert_eq!(u8::from(Socks5ReplyCode::Succeeded), 0x00);
        assert_eq!(u8::from(Socks5ReplyCode::GeneralFailure), 0x01);
        assert_eq!(u8::from(Socks5ReplyCode::ConnectionRefused), 0x05);
    }

    #[test]
    fn test_socks5_reply_code_from_io_error() {
        let err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(Socks5ReplyCode::from(&err), Socks5ReplyCode::ConnectionRefused);

        let err = io::Error::new(io::ErrorKind::TimedOut, "timeout");
        assert_eq!(Socks5ReplyCode::from(&err), Socks5ReplyCode::HostUnreachable);

        let err = io::Error::new(io::ErrorKind::Other, "other");
        assert_eq!(Socks5ReplyCode::from(&err), Socks5ReplyCode::GeneralFailure);
    }

    #[test]
    fn test_socksrat_error_display() {
        let err = SocksRatError::Config("invalid config".to_string());
        assert_eq!(format!("{}", err), "Configuration error: invalid config");

        let err = SocksRatError::Protocol("bad protocol".to_string());
        assert_eq!(format!("{}", err), "Protocol error: bad protocol");
    }

    #[test]
    fn test_socks5_error_display() {
        let err = Socks5Error::UnsupportedVersion(4);
        assert_eq!(format!("{}", err), "Unsupported SOCKS version: 4");

        let err = Socks5Error::NoAcceptableMethod;
        assert_eq!(format!("{}", err), "No acceptable authentication method");
    }
}
