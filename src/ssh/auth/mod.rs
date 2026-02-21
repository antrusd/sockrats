//! SSH authentication module
//!
//! This module provides authentication mechanisms for the SSH server.

pub mod authorized_keys;
pub mod password;
pub mod publickey;

pub use authorized_keys::AuthorizedKeys;
pub use password::verify_password;
pub use publickey::PublicKeyAuth;

#[cfg(feature = "ssh")]
pub use publickey::verify_public_key;

/// Authentication result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthResult {
    /// Authentication successful
    Success,
    /// Authentication failed
    Failure,
    /// Partial authentication (more methods required)
    Partial,
}

impl AuthResult {
    /// Check if authentication was successful
    pub fn is_success(&self) -> bool {
        matches!(self, AuthResult::Success)
    }

    /// Check if authentication failed
    pub fn is_failure(&self) -> bool {
        matches!(self, AuthResult::Failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_result_success() {
        let result = AuthResult::Success;
        assert!(result.is_success());
        assert!(!result.is_failure());
    }

    #[test]
    fn test_auth_result_failure() {
        let result = AuthResult::Failure;
        assert!(!result.is_success());
        assert!(result.is_failure());
    }

    #[test]
    fn test_auth_result_partial() {
        let result = AuthResult::Partial;
        assert!(!result.is_success());
        assert!(!result.is_failure());
    }

    #[test]
    fn test_auth_result_eq() {
        assert_eq!(AuthResult::Success, AuthResult::Success);
        assert_eq!(AuthResult::Failure, AuthResult::Failure);
        assert_eq!(AuthResult::Partial, AuthResult::Partial);
        assert_ne!(AuthResult::Success, AuthResult::Failure);
    }

    #[test]
    fn test_auth_result_clone() {
        let result = AuthResult::Success;
        let cloned = result.clone();
        assert_eq!(result, cloned);
    }

    #[test]
    fn test_auth_result_debug() {
        let result = AuthResult::Success;
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Success"));
    }
}
