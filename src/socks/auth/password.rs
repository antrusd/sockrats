//! Username/password authentication handler
//!
//! Implements RFC 1929 username/password authentication for SOCKS5.

use crate::config::SocksConfig;
use crate::socks::consts::SOCKS5_AUTH_VERSION;
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
}
