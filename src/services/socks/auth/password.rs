//! Username/password authentication handler
//!
//! Implements RFC 1929 username/password authentication for SOCKS5.

use crate::config::SocksConfig;
use crate::services::socks::consts::SOCKS5_AUTH_VERSION;
use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Username/password authentication handler
pub struct PasswordAuth;

/// Authentication result codes
const AUTH_SUCCESS: u8 = 0x00;
const AUTH_FAILURE: u8 = 0x01;

impl PasswordAuth {
    /// Perform username/password authentication
    ///
    /// # Protocol
    ///
    /// Client sends:
    /// ```text
    /// +----+------+----------+------+----------+
    /// |VER | ULEN |  UNAME   | PLEN |  PASSWD  |
    /// +----+------+----------+------+----------+
    /// | 1  |  1   | 1 to 255 |  1   | 1 to 255 |
    /// +----+------+----------+------+----------+
    /// ```
    ///
    /// Server responds:
    /// ```text
    /// +----+--------+
    /// |VER | STATUS |
    /// +----+--------+
    /// | 1  |   1    |
    /// +----+--------+
    /// ```
    pub async fn authenticate<S>(
        stream: &mut S,
        expected_username: &str,
        expected_password: &str,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Read version and username length
        let mut buf = [0u8; 2];
        stream.read_exact(&mut buf).await?;

        let version = buf[0];
        let username_len = buf[1] as usize;

        if version != SOCKS5_AUTH_VERSION {
            send_auth_result(stream, AUTH_FAILURE).await?;
            bail!("Invalid auth version: {}", version);
        }

        if username_len == 0 || username_len > 255 {
            send_auth_result(stream, AUTH_FAILURE).await?;
            bail!("Invalid username length: {}", username_len);
        }

        // Read username
        let mut username = vec![0u8; username_len];
        stream.read_exact(&mut username).await?;
        let username = String::from_utf8(username)?;

        // Read password length
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf).await?;
        let password_len = buf[0] as usize;

        if password_len == 0 || password_len > 255 {
            send_auth_result(stream, AUTH_FAILURE).await?;
            bail!("Invalid password length: {}", password_len);
        }

        // Read password
        let mut password = vec![0u8; password_len];
        stream.read_exact(&mut password).await?;
        let password = String::from_utf8(password)?;

        // Verify credentials
        if username == expected_username && password == expected_password {
            send_auth_result(stream, AUTH_SUCCESS).await?;
            tracing::debug!("Authentication successful for user: {}", username);
            Ok(())
        } else {
            send_auth_result(stream, AUTH_FAILURE).await?;
            bail!("Authentication failed for user: {}", username);
        }
    }
}

/// Send authentication result to client
async fn send_auth_result<S: AsyncWrite + Unpin>(stream: &mut S, status: u8) -> Result<()> {
    stream.write_all(&[SOCKS5_AUTH_VERSION, status]).await?;
    stream.flush().await?;
    Ok(())
}

/// Perform password authentication using SocksConfig
pub async fn authenticate_password<S>(stream: &mut S, config: &SocksConfig) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let username = config
        .username
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Username not configured"))?;
    let password = config
        .password
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Password not configured"))?;

    PasswordAuth::authenticate(stream, username, password).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_auth_request(username: &str, password: &str) -> Vec<u8> {
        let mut request = Vec::new();
        request.push(SOCKS5_AUTH_VERSION);
        request.push(username.len() as u8);
        request.extend_from_slice(username.as_bytes());
        request.push(password.len() as u8);
        request.extend_from_slice(password.as_bytes());
        request
    }

    #[test]
    fn test_password_auth_request_format() {
        // Test that the request format is correct
        let request = create_auth_request("user", "pass");
        assert_eq!(request[0], SOCKS5_AUTH_VERSION);
        assert_eq!(request[1], 4); // "user" length
        assert_eq!(&request[2..6], b"user");
        assert_eq!(request[6], 4); // "pass" length
        assert_eq!(&request[7..11], b"pass");
    }

    #[test]
    fn test_create_auth_request_format() {
        let request = create_auth_request("admin", "secret123");

        assert_eq!(request[0], SOCKS5_AUTH_VERSION);
        assert_eq!(request[1], 5); // "admin" length
        assert_eq!(&request[2..7], b"admin");
        assert_eq!(request[7], 9); // "secret123" length
        assert_eq!(&request[8..17], b"secret123");
    }

    #[test]
    fn test_auth_result_codes() {
        assert_eq!(AUTH_SUCCESS, 0);
        assert_eq!(AUTH_FAILURE, 1);
    }

    #[tokio::test]
    async fn test_authenticate_success() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let request = create_auth_request("testuser", "testpass");

        // Write request from client
        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "testuser", "testpass").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_authenticate_wrong_password() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let request = create_auth_request("user", "wrongpass");

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "user", "correctpass").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Authentication failed"));
    }

    #[tokio::test]
    async fn test_authenticate_wrong_username() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let request = create_auth_request("wronguser", "pass");

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "correctuser", "pass").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Authentication failed"));
    }

    #[tokio::test]
    async fn test_authenticate_invalid_version() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let mut request = Vec::new();
        request.push(0xFF); // Invalid version
        request.push(4);
        request.extend_from_slice(b"user");
        request.push(4);
        request.extend_from_slice(b"pass");

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "user", "pass").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid auth version"));
    }

    #[tokio::test]
    async fn test_authenticate_zero_username_length() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let mut request = Vec::new();
        request.push(SOCKS5_AUTH_VERSION);
        request.push(0); // Zero username length
        request.push(4);
        request.extend_from_slice(b"pass");

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "user", "pass").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid username length"));
    }

    #[tokio::test]
    async fn test_authenticate_zero_password_length() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let mut request = Vec::new();
        request.push(SOCKS5_AUTH_VERSION);
        request.push(4);
        request.extend_from_slice(b"user");
        request.push(0); // Zero password length

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let result = PasswordAuth::authenticate(&mut server, "user", "pass").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid password length"));
    }

    #[tokio::test]
    async fn test_authenticate_password_with_config_success() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let request = create_auth_request("myuser", "mypass");

        use tokio::io::AsyncWriteExt;
        client.write_all(&request).await.unwrap();

        let config = SocksConfig {
            username: Some("myuser".to_string()),
            password: Some("mypass".to_string()),
            auth_required: true,
            dns_resolve: true,
            allow_udp: false,
            request_timeout: 10,
        };

        let result = authenticate_password(&mut server, &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_authenticate_password_no_username_in_config() {
        let mut stream = tokio::io::duplex(1024).0;

        let config = SocksConfig {
            username: None,
            password: Some("pass".to_string()),
            auth_required: true,
            dns_resolve: true,
            allow_udp: false,
            request_timeout: 10,
        };

        let result = authenticate_password(&mut stream, &config).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Username not configured"));
    }

    #[tokio::test]
    async fn test_authenticate_password_no_password_in_config() {
        let mut stream = tokio::io::duplex(1024).0;

        let config = SocksConfig {
            username: Some("user".to_string()),
            password: None,
            auth_required: true,
            dns_resolve: true,
            allow_udp: false,
            request_timeout: 10,
        };

        let result = authenticate_password(&mut stream, &config).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Password not configured"));
    }

    #[tokio::test]
    async fn test_send_auth_result_success() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        // Spawn task to send auth result
        let send_task =
            tokio::spawn(async move { send_auth_result(&mut server, AUTH_SUCCESS).await });

        // Read the response
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 2];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], SOCKS5_AUTH_VERSION);
        assert_eq!(buf[1], AUTH_SUCCESS);

        assert!(send_task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_send_auth_result_failure() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        // Spawn task to send auth result
        let send_task =
            tokio::spawn(async move { send_auth_result(&mut server, AUTH_FAILURE).await });

        // Read the response
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 2];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], SOCKS5_AUTH_VERSION);
        assert_eq!(buf[1], AUTH_FAILURE);

        assert!(send_task.await.unwrap().is_ok());
    }
}
