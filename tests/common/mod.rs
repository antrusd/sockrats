//! Test utilities and mocks for Sockrats
//!
//! This module provides common test utilities used across integration tests.

use std::net::SocketAddr;
use tokio::io::{duplex, DuplexStream};
use tokio::net::{TcpListener, TcpStream};

/// Create a pair of connected duplex streams for testing
pub fn create_mock_stream_pair() -> (DuplexStream, DuplexStream) {
    duplex(8192)
}

/// Create a test TCP listener on an available port
pub async fn create_test_listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, addr)
}

/// Create a connected TCP stream pair for testing
pub async fn create_tcp_stream_pair() -> (TcpStream, TcpStream) {
    let (listener, addr) = create_test_listener().await;

    let connect_fut = TcpStream::connect(addr);
    let accept_fut = listener.accept();

    let (client_stream, (server_stream, _)) = tokio::join!(connect_fut, accept_fut);

    (client_stream.unwrap(), server_stream.unwrap())
}

/// Test configuration builder
pub struct TestConfigBuilder {
    remote_addr: String,
    service_name: String,
    token: String,
    auth_required: bool,
    allow_udp: bool,
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        TestConfigBuilder {
            remote_addr: "127.0.0.1:2333".to_string(),
            service_name: "test-socks".to_string(),
            token: "test-token".to_string(),
            auth_required: false,
            allow_udp: false,
        }
    }
}

impl TestConfigBuilder {
    /// Create a new test config builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set remote address
    pub fn remote_addr(mut self, addr: &str) -> Self {
        self.remote_addr = addr.to_string();
        self
    }

    /// Set service name
    pub fn service_name(mut self, name: &str) -> Self {
        self.service_name = name.to_string();
        self
    }

    /// Set token
    pub fn token(mut self, token: &str) -> Self {
        self.token = token.to_string();
        self
    }

    /// Set authentication required
    pub fn auth_required(mut self, required: bool) -> Self {
        self.auth_required = required;
        self
    }

    /// Allow UDP
    pub fn allow_udp(mut self, allow: bool) -> Self {
        self.allow_udp = allow;
        self
    }

    /// Build the configuration
    pub fn build(self) -> sockrats::config::Config {
        sockrats::config::Config {
            client: sockrats::config::ClientConfig {
                remote_addr: self.remote_addr,
                service_name: self.service_name,
                token: self.token,
                transport: sockrats::config::TransportConfig::default(),
                heartbeat_timeout: 40,
                socks: sockrats::config::SocksConfig {
                    auth_required: self.auth_required,
                    username: if self.auth_required {
                        Some("testuser".to_string())
                    } else {
                        None
                    },
                    password: if self.auth_required {
                        Some("testpass".to_string())
                    } else {
                        None
                    },
                    allow_udp: self.allow_udp,
                    dns_resolve: true,
                    request_timeout: 10,
                },
                pool: sockrats::config::PoolConfig::default(),
            },
        }
    }
}

/// Mock SOCKS5 handshake data
pub mod socks5_mock {
    use sockrats::socks::*;

    /// Create a no-auth method selection request
    pub fn create_auth_request_no_auth() -> Vec<u8> {
        vec![SOCKS5_VERSION, 1, SOCKS5_AUTH_METHOD_NONE]
    }

    /// Create a password auth method selection request
    pub fn create_auth_request_password() -> Vec<u8> {
        vec![SOCKS5_VERSION, 1, SOCKS5_AUTH_METHOD_PASSWORD]
    }

    /// Create a connect command to IPv4 address
    pub fn create_connect_ipv4(ip: [u8; 4], port: u16) -> Vec<u8> {
        let mut cmd = vec![
            SOCKS5_VERSION,
            SOCKS5_CMD_TCP_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ADDR_TYPE_IPV4,
        ];
        cmd.extend_from_slice(&ip);
        cmd.extend_from_slice(&port.to_be_bytes());
        cmd
    }

    /// Create a connect command to domain
    pub fn create_connect_domain(domain: &str, port: u16) -> Vec<u8> {
        let mut cmd = vec![
            SOCKS5_VERSION,
            SOCKS5_CMD_TCP_CONNECT,
            SOCKS5_RESERVED,
            SOCKS5_ADDR_TYPE_DOMAIN,
            domain.len() as u8,
        ];
        cmd.extend_from_slice(domain.as_bytes());
        cmd.extend_from_slice(&port.to_be_bytes());
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_mock_stream_pair() {
        let (mut a, mut b) = create_mock_stream_pair();

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        a.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        b.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[tokio::test]
    async fn test_create_test_listener() {
        let (listener, addr) = create_test_listener().await;
        assert!(addr.port() > 0);
        drop(listener);
    }

    #[test]
    fn test_config_builder() {
        let config = TestConfigBuilder::new()
            .remote_addr("192.168.1.1:1234")
            .service_name("my-service")
            .token("my-token")
            .auth_required(true)
            .allow_udp(true)
            .build();

        assert_eq!(config.client.remote_addr, "192.168.1.1:1234");
        assert_eq!(config.client.service_name, "my-service");
        assert!(config.client.socks.auth_required);
        assert!(config.client.socks.allow_udp);
    }

    #[test]
    fn test_socks5_mock_auth_request() {
        let request = socks5_mock::create_auth_request_no_auth();
        assert_eq!(request[0], 5); // SOCKS5 version
        assert_eq!(request[1], 1); // 1 method
        assert_eq!(request[2], 0); // NO AUTH
    }

    #[test]
    fn test_socks5_mock_connect_ipv4() {
        let cmd = socks5_mock::create_connect_ipv4([192, 168, 1, 1], 8080);
        assert_eq!(cmd[0], 5); // SOCKS5 version
        assert_eq!(cmd[1], 1); // CONNECT
        assert_eq!(cmd[3], 1); // IPv4
        assert_eq!(&cmd[4..8], &[192, 168, 1, 1]);
    }
}
