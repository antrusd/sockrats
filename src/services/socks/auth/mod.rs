//! SOCKS5 authentication module
//!
//! Handles authentication negotiation and username/password authentication.

mod none;
mod password;

#[allow(unused_imports)]
pub use none::NoAuth;
#[allow(unused_imports)]
pub use password::PasswordAuth;

use super::consts::*;
use crate::config::SocksConfig;
use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Authentication method types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// No authentication required
    None,
    /// Username/password authentication
    Password,
}

impl AuthMethod {
    /// Convert to SOCKS5 method byte
    pub fn to_byte(self) -> u8 {
        match self {
            AuthMethod::None => SOCKS5_AUTH_METHOD_NONE,
            AuthMethod::Password => SOCKS5_AUTH_METHOD_PASSWORD,
        }
    }

    /// Parse from SOCKS5 method byte
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            SOCKS5_AUTH_METHOD_NONE => Some(AuthMethod::None),
            SOCKS5_AUTH_METHOD_PASSWORD => Some(AuthMethod::Password),
            _ => None,
        }
    }
}

/// Perform authentication negotiation and authentication
///
/// This function handles the complete SOCKS5 authentication flow:
/// 1. Read client's supported methods
/// 2. Select appropriate method based on configuration
/// 3. Perform authentication if required
///
/// # Arguments
///
/// * `stream` - The stream to authenticate
/// * `config` - SOCKS5 configuration
///
/// # Returns
///
/// The selected authentication method if successful
pub async fn authenticate<S>(stream: &mut S, config: &SocksConfig) -> Result<AuthMethod>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Step 1: Read version and number of methods
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let num_methods = buf[1];

    if version != SOCKS5_VERSION {
        bail!("Unsupported SOCKS version: {}", version);
    }

    if num_methods == 0 {
        bail!("No authentication methods provided");
    }

    // Step 2: Read available methods
    let mut methods = vec![0u8; num_methods as usize];
    stream.read_exact(&mut methods).await?;

    // Step 3: Select authentication method
    let selected_method = select_auth_method(&methods, config);

    // Step 4: Send selected method
    stream
        .write_all(&[
            SOCKS5_VERSION,
            selected_method
                .map(|m| m.to_byte())
                .unwrap_or(SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE),
        ])
        .await?;
    stream.flush().await?;

    let method = match selected_method {
        Some(m) => m,
        None => bail!("No acceptable authentication method"),
    };

    // Step 5: Perform authentication if required
    if method == AuthMethod::Password {
        password::authenticate_password(stream, config).await?;
    }

    Ok(method)
}

/// Select the best authentication method based on configuration and available methods
fn select_auth_method(methods: &[u8], config: &SocksConfig) -> Option<AuthMethod> {
    if config.auth_required {
        // Must use password authentication
        if methods.contains(&SOCKS5_AUTH_METHOD_PASSWORD) {
            return Some(AuthMethod::Password);
        }
    } else {
        // Prefer no authentication, but allow password if configured
        if methods.contains(&SOCKS5_AUTH_METHOD_NONE) {
            return Some(AuthMethod::None);
        }
        if methods.contains(&SOCKS5_AUTH_METHOD_PASSWORD) && config.has_credentials() {
            return Some(AuthMethod::Password);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_method_to_byte() {
        assert_eq!(AuthMethod::None.to_byte(), SOCKS5_AUTH_METHOD_NONE);
        assert_eq!(AuthMethod::Password.to_byte(), SOCKS5_AUTH_METHOD_PASSWORD);
    }

    #[test]
    fn test_auth_method_from_byte() {
        assert_eq!(AuthMethod::from_byte(0), Some(AuthMethod::None));
        assert_eq!(AuthMethod::from_byte(2), Some(AuthMethod::Password));
        assert_eq!(AuthMethod::from_byte(1), None); // GSSAPI not implemented
        assert_eq!(AuthMethod::from_byte(255), None);
    }

    #[test]
    fn test_select_auth_method_no_auth_required() {
        let config = SocksConfig {
            auth_required: false,
            username: None,
            password: None,
            ..Default::default()
        };

        // Should select no auth when available
        let methods = vec![SOCKS5_AUTH_METHOD_NONE, SOCKS5_AUTH_METHOD_PASSWORD];
        assert_eq!(
            select_auth_method(&methods, &config),
            Some(AuthMethod::None)
        );

        // Should return None if only password and no credentials
        let methods = vec![SOCKS5_AUTH_METHOD_PASSWORD];
        assert_eq!(select_auth_method(&methods, &config), None);
    }

    #[test]
    fn test_select_auth_method_auth_required() {
        let config = SocksConfig {
            auth_required: true,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };

        // Should select password auth
        let methods = vec![SOCKS5_AUTH_METHOD_NONE, SOCKS5_AUTH_METHOD_PASSWORD];
        assert_eq!(
            select_auth_method(&methods, &config),
            Some(AuthMethod::Password)
        );

        // Should return None if password not available
        let methods = vec![SOCKS5_AUTH_METHOD_NONE];
        assert_eq!(select_auth_method(&methods, &config), None);
    }

    #[test]
    fn test_select_auth_method_with_credentials_no_requirement() {
        let config = SocksConfig {
            auth_required: false,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };

        // Should prefer no auth even with credentials
        let methods = vec![SOCKS5_AUTH_METHOD_NONE, SOCKS5_AUTH_METHOD_PASSWORD];
        assert_eq!(
            select_auth_method(&methods, &config),
            Some(AuthMethod::None)
        );

        // But should use password if no auth not available
        let methods = vec![SOCKS5_AUTH_METHOD_PASSWORD];
        assert_eq!(
            select_auth_method(&methods, &config),
            Some(AuthMethod::Password)
        );
    }
}
