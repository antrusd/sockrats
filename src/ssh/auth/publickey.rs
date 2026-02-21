//! Public key authentication for SSH
//!
//! This module handles public key-based SSH authentication.

use super::authorized_keys::AuthorizedKeys;
#[cfg(feature = "ssh")]
use super::super::config::SshConfig;
#[cfg(feature = "ssh")]
use std::collections::HashMap;

#[cfg(feature = "ssh")]
use russh::keys::PublicKey;

/// Public key authenticator
#[derive(Debug)]
pub struct PublicKeyAuth {
    authorized_keys: AuthorizedKeys,
}

impl PublicKeyAuth {
    /// Create a new public key authenticator from config
    #[cfg(feature = "ssh")]
    pub fn from_config(config: &SshConfig) -> anyhow::Result<Option<Self>> {
        if !config.has_publickey_auth() {
            return Ok(None);
        }

        let authorized_keys_path = match &config.authorized_keys {
            Some(path) => path,
            None => {
                tracing::warn!("Public key auth enabled but no authorized_keys path configured");
                return Ok(None);
            }
        };

        let authorized_keys = AuthorizedKeys::from_file(authorized_keys_path)?;

        if authorized_keys.is_empty() {
            tracing::warn!("No authorized keys found in {:?}", authorized_keys_path);
        }

        Ok(Some(Self { authorized_keys }))
    }

    /// Create a public key authenticator from an AuthorizedKeys collection
    pub fn new(authorized_keys: AuthorizedKeys) -> Self {
        Self { authorized_keys }
    }

    /// Check if a public key is authorized
    #[cfg(feature = "ssh")]
    pub fn is_authorized(&self, key: &PublicKey) -> bool {
        self.authorized_keys.is_authorized(key)
    }

    /// Get options for an authorized key
    #[cfg(feature = "ssh")]
    pub fn get_options(&self, key: &PublicKey) -> Option<&HashMap<String, Option<String>>> {
        self.authorized_keys.get_options(key)
    }

    /// Get the number of authorized keys
    pub fn num_keys(&self) -> usize {
        self.authorized_keys.len()
    }
}

/// Verify a public key against authorized keys
#[cfg(feature = "ssh")]
pub fn verify_public_key(
    auth: Option<&PublicKeyAuth>,
    config: &SshConfig,
    key: &PublicKey,
) -> bool {
    // Check if public key auth is enabled
    if !config.has_publickey_auth() {
        tracing::debug!("Public key authentication is not enabled");
        return false;
    }

    match auth {
        Some(auth) => {
            if auth.is_authorized(key) {
                tracing::info!("Public key authentication successful");
                true
            } else {
                tracing::warn!("Public key not found in authorized_keys");
                false
            }
        }
        None => {
            tracing::warn!("Public key auth enabled but no authenticator configured");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_key_auth_new() {
        let authorized_keys = AuthorizedKeys::new();
        let auth = PublicKeyAuth::new(authorized_keys);
        assert_eq!(auth.num_keys(), 0);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_public_key_auth_from_config_disabled() {
        let config = SshConfig {
            enabled: true,
            auth_methods: vec!["password".to_string()],
            ..Default::default()
        };

        let auth = PublicKeyAuth::from_config(&config).unwrap();
        assert!(auth.is_none());
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_public_key_auth_from_config_no_path() {
        let config = SshConfig {
            enabled: true,
            auth_methods: vec!["publickey".to_string()],
            authorized_keys: None,
            ..Default::default()
        };

        let auth = PublicKeyAuth::from_config(&config).unwrap();
        assert!(auth.is_none());
    }

    #[test]
    fn test_module_compiles_without_ssh_feature() {
        let authorized_keys = AuthorizedKeys::new();
        let auth = PublicKeyAuth::new(authorized_keys);
        assert_eq!(auth.num_keys(), 0);
    }
}
