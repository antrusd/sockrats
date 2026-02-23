//! Password authentication for SSH
//!
//! This module handles password-based SSH authentication.

use super::super::config::SshConfig;

/// Verify a password against the configured credentials
pub fn verify_password(config: &SshConfig, username: &str, password: &str) -> bool {
    // Check if password auth is enabled
    if !config.has_password_auth() {
        tracing::debug!("Password authentication is not enabled");
        return false;
    }

    // Get configured credentials
    let expected_username = match &config.username {
        Some(u) => u,
        None => {
            tracing::warn!("Password auth enabled but no username configured");
            return false;
        }
    };

    let expected_password = match &config.password {
        Some(p) => p,
        None => {
            tracing::warn!("Password auth enabled but no password configured");
            return false;
        }
    };

    // Constant-time comparison to prevent timing attacks
    let username_matches = constant_time_compare(username.as_bytes(), expected_username.as_bytes());
    let password_matches = constant_time_compare(password.as_bytes(), expected_password.as_bytes());

    if username_matches && password_matches {
        tracing::info!(username = %username, "Password authentication successful");
        true
    } else {
        tracing::warn!(username = %username, "Password authentication failed");
        false
    }
}

/// Constant-time comparison of two byte slices
///
/// This prevents timing attacks by ensuring the comparison takes
/// the same amount of time regardless of where the mismatch occurs.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_config_with_password(username: &str, password: &str) -> SshConfig {
        SshConfig {
            enabled: true,
            auth_methods: vec!["password".to_string()],
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_verify_password_success() {
        let config = create_config_with_password("admin", "secret123");
        assert!(verify_password(&config, "admin", "secret123"));
    }

    #[test]
    fn test_verify_password_wrong_password() {
        let config = create_config_with_password("admin", "secret123");
        assert!(!verify_password(&config, "admin", "wrongpass"));
    }

    #[test]
    fn test_verify_password_wrong_username() {
        let config = create_config_with_password("admin", "secret123");
        assert!(!verify_password(&config, "wrong", "secret123"));
    }

    #[test]
    fn test_verify_password_both_wrong() {
        let config = create_config_with_password("admin", "secret123");
        assert!(!verify_password(&config, "wrong", "wrongpass"));
    }

    #[test]
    fn test_verify_password_empty_credentials() {
        let config = create_config_with_password("", "");
        assert!(verify_password(&config, "", ""));
    }

    #[test]
    fn test_verify_password_disabled() {
        let mut config = create_config_with_password("admin", "secret123");
        config.auth_methods = vec!["publickey".to_string()];
        assert!(!verify_password(&config, "admin", "secret123"));
    }

    #[test]
    fn test_verify_password_no_username_configured() {
        let mut config = create_config_with_password("admin", "secret123");
        config.username = None;
        assert!(!verify_password(&config, "admin", "secret123"));
    }

    #[test]
    fn test_verify_password_no_password_configured() {
        let mut config = create_config_with_password("admin", "secret123");
        config.password = None;
        assert!(!verify_password(&config, "admin", "secret123"));
    }

    #[test]
    fn test_constant_time_compare_equal() {
        assert!(constant_time_compare(b"hello", b"hello"));
        assert!(constant_time_compare(b"", b""));
        assert!(constant_time_compare(b"a", b"a"));
    }

    #[test]
    fn test_constant_time_compare_not_equal() {
        assert!(!constant_time_compare(b"hello", b"world"));
        assert!(!constant_time_compare(b"hello", b"hell"));
        assert!(!constant_time_compare(b"a", b"b"));
    }

    #[test]
    fn test_constant_time_compare_different_lengths() {
        assert!(!constant_time_compare(b"short", b"longer"));
        assert!(!constant_time_compare(b"hello", b""));
    }

    #[test]
    fn test_constant_time_compare_binary() {
        assert!(constant_time_compare(&[0, 1, 2, 3], &[0, 1, 2, 3]));
        assert!(!constant_time_compare(&[0, 1, 2, 3], &[0, 1, 2, 4]));
    }
}
