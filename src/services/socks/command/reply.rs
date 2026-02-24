//! SOCKS5 reply builder
//!
//! Constructs SOCKS5 reply messages.

use crate::services::socks::consts::*;
use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Build and send a SOCKS5 reply
///
/// # SOCKS5 Reply Format
///
/// ```text
/// +----+-----+-------+------+----------+----------+
/// |VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
/// +----+-----+-------+------+----------+----------+
/// | 1  |  1  | X'00' |  1   | Variable |    2     |
/// +----+-----+-------+------+----------+----------+
/// ```
///
/// # Arguments
///
/// * `stream` - The stream to write to
/// * `reply_code` - The reply status code
/// * `bind_addr` - The bound address (optional, defaults to 0.0.0.0:0)
pub async fn build_reply<S>(
    stream: &mut S,
    reply_code: u8,
    bind_addr: Option<SocketAddr>,
) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let bind_addr =
        bind_addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0));

    let mut reply = vec![SOCKS5_VERSION, reply_code, SOCKS5_RESERVED];

    // Add address
    match bind_addr {
        SocketAddr::V4(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV4);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV6);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
    }

    stream.write_all(&reply).await?;
    stream.flush().await?;

    Ok(())
}

/// Build a success reply
pub async fn send_success<S>(stream: &mut S, bind_addr: Option<SocketAddr>) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    build_reply(stream, SOCKS5_REPLY_SUCCEEDED, bind_addr).await
}

/// Build an error reply from an IO error
pub async fn send_io_error<S>(stream: &mut S, error: &std::io::Error) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let reply_code = match error.kind() {
        std::io::ErrorKind::ConnectionRefused => SOCKS5_REPLY_CONNECTION_REFUSED,
        std::io::ErrorKind::TimedOut => SOCKS5_REPLY_HOST_UNREACHABLE,
        std::io::ErrorKind::AddrNotAvailable => SOCKS5_REPLY_HOST_UNREACHABLE,
        std::io::ErrorKind::PermissionDenied => SOCKS5_REPLY_CONNECTION_NOT_ALLOWED,
        _ => SOCKS5_REPLY_GENERAL_FAILURE,
    };

    build_reply(stream, reply_code, None).await
}

/// Build a "command not supported" reply
pub async fn send_command_not_supported<S>(stream: &mut S) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    build_reply(stream, SOCKS5_REPLY_COMMAND_NOT_SUPPORTED, None).await
}

/// Build a "general failure" reply
pub async fn send_general_failure<S>(stream: &mut S) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    build_reply(stream, SOCKS5_REPLY_GENERAL_FAILURE, None).await
}

/// Build reply bytes without sending (used in tests)
#[cfg(test)]
fn build_reply_bytes(reply_code: u8, bind_addr: Option<SocketAddr>) -> Vec<u8> {
    let bind_addr =
        bind_addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0));

    let mut reply = vec![SOCKS5_VERSION, reply_code, SOCKS5_RESERVED];

    match bind_addr {
        SocketAddr::V4(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV4);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV6);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
    }

    reply
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    #[test]
    fn test_build_reply_bytes_ipv4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 8080);
        let reply = build_reply_bytes(SOCKS5_REPLY_SUCCEEDED, Some(addr));

        assert_eq!(reply[0], SOCKS5_VERSION);
        assert_eq!(reply[1], SOCKS5_REPLY_SUCCEEDED);
        assert_eq!(reply[2], SOCKS5_RESERVED);
        assert_eq!(reply[3], SOCKS5_ADDR_TYPE_IPV4);
        assert_eq!(&reply[4..8], &[192, 168, 1, 1]);
        assert_eq!(&reply[8..10], &8080u16.to_be_bytes());
    }

    #[test]
    fn test_build_reply_bytes_ipv6() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 443);
        let reply = build_reply_bytes(SOCKS5_REPLY_SUCCEEDED, Some(addr));

        assert_eq!(reply[0], SOCKS5_VERSION);
        assert_eq!(reply[1], SOCKS5_REPLY_SUCCEEDED);
        assert_eq!(reply[2], SOCKS5_RESERVED);
        assert_eq!(reply[3], SOCKS5_ADDR_TYPE_IPV6);
        assert_eq!(reply.len(), 3 + 1 + 16 + 2); // header + atyp + ipv6 + port
    }

    #[test]
    fn test_build_reply_bytes_default_addr() {
        let reply = build_reply_bytes(SOCKS5_REPLY_GENERAL_FAILURE, None);

        assert_eq!(reply[0], SOCKS5_VERSION);
        assert_eq!(reply[1], SOCKS5_REPLY_GENERAL_FAILURE);
        assert_eq!(reply[3], SOCKS5_ADDR_TYPE_IPV4);
        assert_eq!(&reply[4..8], &[0, 0, 0, 0]); // 0.0.0.0
        assert_eq!(&reply[8..10], &[0, 0]); // port 0
    }

    #[test]
    fn test_build_reply_bytes_various_codes() {
        let codes = [
            SOCKS5_REPLY_SUCCEEDED,
            SOCKS5_REPLY_GENERAL_FAILURE,
            SOCKS5_REPLY_CONNECTION_REFUSED,
            SOCKS5_REPLY_HOST_UNREACHABLE,
            SOCKS5_REPLY_COMMAND_NOT_SUPPORTED,
        ];

        for code in codes {
            let reply = build_reply_bytes(code, None);
            assert_eq!(reply[1], code);
        }
    }

    #[tokio::test]
    async fn test_build_reply_async() {
        let mut buffer = Vec::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080);

        build_reply(&mut buffer, SOCKS5_REPLY_SUCCEEDED, Some(addr))
            .await
            .unwrap();

        assert_eq!(buffer[0], SOCKS5_VERSION);
        assert_eq!(buffer[1], SOCKS5_REPLY_SUCCEEDED);
        assert_eq!(buffer[3], SOCKS5_ADDR_TYPE_IPV4);
    }

    #[tokio::test]
    async fn test_send_success() {
        let mut buffer = Vec::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 9090);

        send_success(&mut buffer, Some(addr)).await.unwrap();

        assert_eq!(buffer[0], SOCKS5_VERSION);
        assert_eq!(buffer[1], SOCKS5_REPLY_SUCCEEDED);
        assert_eq!(buffer[3], SOCKS5_ADDR_TYPE_IPV4);
        assert_eq!(&buffer[4..8], &[10, 0, 0, 1]);
    }

    #[tokio::test]
    async fn test_send_io_error_connection_refused() {
        let mut buffer = Vec::new();
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");

        send_io_error(&mut buffer, &err).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_CONNECTION_REFUSED);
    }

    #[tokio::test]
    async fn test_send_io_error_timed_out() {
        let mut buffer = Vec::new();
        let err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");

        send_io_error(&mut buffer, &err).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_HOST_UNREACHABLE);
    }

    #[tokio::test]
    async fn test_send_io_error_permission_denied() {
        let mut buffer = Vec::new();
        let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");

        send_io_error(&mut buffer, &err).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_CONNECTION_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_send_io_error_other() {
        let mut buffer = Vec::new();
        let err = std::io::Error::new(std::io::ErrorKind::Other, "other");

        send_io_error(&mut buffer, &err).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_GENERAL_FAILURE);
    }

    #[tokio::test]
    async fn test_send_command_not_supported() {
        let mut buffer = Vec::new();

        send_command_not_supported(&mut buffer).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_COMMAND_NOT_SUPPORTED);
    }

    #[tokio::test]
    async fn test_send_general_failure() {
        let mut buffer = Vec::new();

        send_general_failure(&mut buffer).await.unwrap();

        assert_eq!(buffer[1], SOCKS5_REPLY_GENERAL_FAILURE);
    }
}
