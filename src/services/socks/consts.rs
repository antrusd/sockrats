//! SOCKS5 protocol constants
//!
//! Defines all constants used in the SOCKS5 protocol implementation.

/// SOCKS5 protocol version
pub const SOCKS5_VERSION: u8 = 0x05;

/// SOCKS5 authentication sub-negotiation version
pub const SOCKS5_AUTH_VERSION: u8 = 0x01;

// Authentication methods
/// No authentication required
pub const SOCKS5_AUTH_METHOD_NONE: u8 = 0x00;
/// GSSAPI authentication (not implemented)
pub const SOCKS5_AUTH_METHOD_GSSAPI: u8 = 0x01;
/// Username/password authentication
pub const SOCKS5_AUTH_METHOD_PASSWORD: u8 = 0x02;
/// No acceptable methods
pub const SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE: u8 = 0xFF;

// Commands
/// TCP CONNECT command
pub const SOCKS5_CMD_TCP_CONNECT: u8 = 0x01;
/// TCP BIND command (not implemented)
pub const SOCKS5_CMD_TCP_BIND: u8 = 0x02;
/// UDP ASSOCIATE command
pub const SOCKS5_CMD_UDP_ASSOCIATE: u8 = 0x03;

// Address types
/// IPv4 address
pub const SOCKS5_ADDR_TYPE_IPV4: u8 = 0x01;
/// Domain name
pub const SOCKS5_ADDR_TYPE_DOMAIN: u8 = 0x03;
/// IPv6 address
pub const SOCKS5_ADDR_TYPE_IPV6: u8 = 0x04;

// Reply codes
/// Succeeded
pub const SOCKS5_REPLY_SUCCEEDED: u8 = 0x00;
/// General SOCKS server failure
pub const SOCKS5_REPLY_GENERAL_FAILURE: u8 = 0x01;
/// Connection not allowed by ruleset
pub const SOCKS5_REPLY_CONNECTION_NOT_ALLOWED: u8 = 0x02;
/// Network unreachable
pub const SOCKS5_REPLY_NETWORK_UNREACHABLE: u8 = 0x03;
/// Host unreachable
pub const SOCKS5_REPLY_HOST_UNREACHABLE: u8 = 0x04;
/// Connection refused
pub const SOCKS5_REPLY_CONNECTION_REFUSED: u8 = 0x05;
/// TTL expired
pub const SOCKS5_REPLY_TTL_EXPIRED: u8 = 0x06;
/// Command not supported
pub const SOCKS5_REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;
/// Address type not supported
pub const SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

// Reserved byte
/// Reserved byte value (always 0x00)
pub const SOCKS5_RESERVED: u8 = 0x00;

// Buffer sizes
/// Maximum domain name length
pub const MAX_DOMAIN_LEN: usize = 255;
/// Default buffer size for data transfer
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks5_version() {
        assert_eq!(SOCKS5_VERSION, 5);
    }

    #[test]
    fn test_auth_methods() {
        assert_eq!(SOCKS5_AUTH_METHOD_NONE, 0);
        assert_eq!(SOCKS5_AUTH_METHOD_PASSWORD, 2);
        assert_eq!(SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE, 255);
    }

    #[test]
    fn test_commands() {
        assert_eq!(SOCKS5_CMD_TCP_CONNECT, 1);
        assert_eq!(SOCKS5_CMD_TCP_BIND, 2);
        assert_eq!(SOCKS5_CMD_UDP_ASSOCIATE, 3);
    }

    #[test]
    fn test_address_types() {
        assert_eq!(SOCKS5_ADDR_TYPE_IPV4, 1);
        assert_eq!(SOCKS5_ADDR_TYPE_DOMAIN, 3);
        assert_eq!(SOCKS5_ADDR_TYPE_IPV6, 4);
    }

    #[test]
    fn test_reply_codes() {
        assert_eq!(SOCKS5_REPLY_SUCCEEDED, 0);
        assert_eq!(SOCKS5_REPLY_GENERAL_FAILURE, 1);
        assert_eq!(SOCKS5_REPLY_COMMAND_NOT_SUPPORTED, 7);
    }
}
