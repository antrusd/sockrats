//! WireGuard tunnel configuration.
//!
//! Defines [`WireguardConfig`] for the `[client.wireguard]` TOML section.
//! WireGuard operates as a separate tunnel layer — not a transport type.
//! When enabled, the transport type **must** be `tcp`.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};

/// Default persistent keepalive interval in seconds.
fn default_keepalive() -> u16 {
    25
}

/// Default client address inside the WireGuard network (CIDR notation).
fn default_address() -> String {
    "10.0.0.2/24".to_string()
}

/// Default allowed IPs for the WireGuard tunnel.
fn default_allowed_ips() -> Vec<String> {
    vec!["10.0.0.0/24".to_string()]
}

/// WireGuard tunnel configuration.
///
/// Lives at `[client.wireguard]` in the TOML config file.
/// When `enabled = true`, the transport type must be `"tcp"` because
/// WireGuard already provides encryption — layering Noise on top would
/// be redundant double-encryption.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WireguardConfig {
    /// Enable or disable the WireGuard tunnel (default: false).
    #[serde(default)]
    pub enabled: bool,

    /// Local WireGuard private key (base64-encoded, 32 bytes decoded).
    pub private_key: String,

    /// Remote peer's public key (base64-encoded, 32 bytes decoded).
    pub peer_public_key: String,

    /// Optional preshared key for post-quantum resistance
    /// (base64-encoded, 32 bytes decoded).
    #[serde(default)]
    pub preshared_key: Option<String>,

    /// Real network endpoint of the WireGuard peer (`host:port` for UDP).
    pub peer_endpoint: String,

    /// Persistent keepalive interval in seconds (0 = disabled, default: 25).
    #[serde(default = "default_keepalive")]
    pub persistent_keepalive: u16,

    /// Virtual IPv4 address for this client in CIDR notation,
    /// matching WireGuard's `[Interface] Address` (default: `"10.0.0.2/24"`).
    #[serde(default = "default_address")]
    pub address: String,

    /// Allowed IP ranges in CIDR notation
    /// (default: `["10.0.0.0/24"]`).
    #[serde(default = "default_allowed_ips")]
    pub allowed_ips: Vec<String>,
}

impl Default for WireguardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            private_key: String::new(),
            peer_public_key: String::new(),
            preshared_key: None,
            peer_endpoint: String::new(),
            persistent_keepalive: default_keepalive(),
            address: default_address(),
            allowed_ips: default_allowed_ips(),
        }
    }
}

impl WireguardConfig {
    /// Validate the configuration, returning an error with a descriptive
    /// message if any field is invalid.
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Validate private key
        Self::validate_key(&self.private_key, "private_key")?;

        // Validate peer public key
        Self::validate_key(&self.peer_public_key, "peer_public_key")?;

        // Validate optional preshared key
        if let Some(ref psk) = self.preshared_key {
            Self::validate_key(psk, "preshared_key")?;
        }

        // Validate peer endpoint
        self.parse_peer_endpoint()
            .context("Invalid peer_endpoint")?;

        // Validate address (CIDR)
        self.parse_address().context("Invalid address")?;

        // Validate allowed IPs (basic CIDR check)
        for cidr in &self.allowed_ips {
            Self::validate_cidr(cidr)?;
        }

        Ok(())
    }

    /// Decode and validate a base64 WireGuard key (must be 32 bytes).
    fn validate_key(key_b64: &str, field_name: &str) -> Result<[u8; 32]> {
        if key_b64.is_empty() {
            bail!("{field_name} must not be empty");
        }
        let decoded = BASE64
            .decode(key_b64)
            .with_context(|| format!("{field_name} is not valid base64"))?;
        if decoded.len() != 32 {
            bail!(
                "{field_name} must decode to exactly 32 bytes, got {}",
                decoded.len()
            );
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&decoded);
        Ok(arr)
    }

    /// Decode the private key to raw 32-byte form.
    pub fn decode_private_key(&self) -> Result<[u8; 32]> {
        Self::validate_key(&self.private_key, "private_key")
    }

    /// Decode the peer public key to raw 32-byte form.
    pub fn decode_peer_public_key(&self) -> Result<[u8; 32]> {
        Self::validate_key(&self.peer_public_key, "peer_public_key")
    }

    /// Decode the optional preshared key to raw 32-byte form.
    pub fn decode_preshared_key(&self) -> Result<Option<[u8; 32]>> {
        match &self.preshared_key {
            Some(psk) => Ok(Some(Self::validate_key(psk, "preshared_key")?)),
            None => Ok(None),
        }
    }

    /// Parse the peer endpoint string into a [`SocketAddr`].
    pub fn parse_peer_endpoint(&self) -> Result<SocketAddr> {
        self.peer_endpoint
            .to_socket_addrs()
            .with_context(|| format!("Cannot resolve peer_endpoint: {}", self.peer_endpoint))?
            .next()
            .with_context(|| {
                format!(
                    "No addresses found for peer_endpoint: {}",
                    self.peer_endpoint
                )
            })
    }

    /// Parse the client address from CIDR notation (e.g. `"10.0.0.2/24"`).
    ///
    /// Returns `(ip, prefix_len)`.
    pub fn parse_address(&self) -> Result<(Ipv4Addr, u8)> {
        let parts: Vec<&str> = self.address.split('/').collect();
        if parts.len() != 2 {
            bail!(
                "Invalid address CIDR notation: {} (expected addr/prefix)",
                self.address
            );
        }
        let ip = parts[0]
            .parse::<Ipv4Addr>()
            .with_context(|| format!("Invalid IP in address: {}", self.address))?;
        let prefix: u8 = parts[1]
            .parse()
            .with_context(|| format!("Invalid prefix in address: {}", self.address))?;
        if prefix > 32 {
            bail!("Address prefix must be 0-32, got {}", prefix);
        }
        Ok((ip, prefix))
    }

    /// Get the keepalive interval, returning `None` when set to 0.
    pub fn keepalive_interval(&self) -> Option<u16> {
        if self.persistent_keepalive == 0 {
            None
        } else {
            Some(self.persistent_keepalive)
        }
    }

    /// Validate a CIDR notation string (e.g. `"10.0.0.0/24"`).
    fn validate_cidr(cidr: &str) -> Result<()> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            bail!("Invalid CIDR notation: {cidr} (expected addr/prefix)");
        }
        parts[0]
            .parse::<Ipv4Addr>()
            .with_context(|| format!("Invalid IP in CIDR: {cidr}"))?;
        let prefix: u8 = parts[1]
            .parse()
            .with_context(|| format!("Invalid prefix length in CIDR: {cidr}"))?;
        if prefix > 32 {
            bail!("CIDR prefix length must be 0-32, got {prefix} in {cidr}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_key_b64() -> String {
        // 32 zero bytes in base64
        BASE64.encode([0u8; 32])
    }

    fn make_valid_config() -> WireguardConfig {
        WireguardConfig {
            enabled: true,
            private_key: valid_key_b64(),
            peer_public_key: valid_key_b64(),
            preshared_key: None,
            peer_endpoint: "127.0.0.1:51820".to_string(),
            persistent_keepalive: 25,
            address: "10.0.0.2/24".to_string(),
            allowed_ips: vec!["10.0.0.0/24".to_string()],
        }
    }

    #[test]
    fn test_default_config() {
        let cfg = WireguardConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.persistent_keepalive, 25);
        assert_eq!(cfg.address, "10.0.0.2/24");
        assert_eq!(cfg.allowed_ips, vec!["10.0.0.0/24".to_string()]);
    }

    #[test]
    fn test_valid_config_validates() {
        let cfg = make_valid_config();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_disabled_config_skips_validation() {
        let cfg = WireguardConfig {
            enabled: false,
            private_key: "bad".to_string(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_invalid_private_key_not_base64() {
        let cfg = WireguardConfig {
            private_key: "not-valid-base64!!!".to_string(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_private_key_wrong_length() {
        let cfg = WireguardConfig {
            private_key: BASE64.encode([0u8; 16]),
            ..make_valid_config()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("32 bytes"));
    }

    #[test]
    fn test_empty_private_key() {
        let cfg = WireguardConfig {
            private_key: String::new(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_peer_endpoint() {
        let cfg = WireguardConfig {
            peer_endpoint: "not-a-valid-endpoint".to_string(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_address() {
        let cfg = WireguardConfig {
            address: "not-an-ip".to_string(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_address_no_prefix() {
        let cfg = WireguardConfig {
            address: "10.0.0.2".to_string(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_address_bad_prefix() {
        let cfg = WireguardConfig {
            address: "10.0.0.2/33".to_string(),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_cidr_no_prefix() {
        let cfg = WireguardConfig {
            allowed_ips: vec!["10.0.0.0".to_string()],
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_cidr_bad_prefix() {
        let cfg = WireguardConfig {
            allowed_ips: vec!["10.0.0.0/33".to_string()],
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_valid_preshared_key() {
        let cfg = WireguardConfig {
            preshared_key: Some(valid_key_b64()),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_invalid_preshared_key() {
        let cfg = WireguardConfig {
            preshared_key: Some("bad-key".to_string()),
            ..make_valid_config()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_keepalive_interval_zero() {
        let cfg = WireguardConfig {
            persistent_keepalive: 0,
            ..make_valid_config()
        };
        assert_eq!(cfg.keepalive_interval(), None);
    }

    #[test]
    fn test_keepalive_interval_nonzero() {
        let cfg = make_valid_config();
        assert_eq!(cfg.keepalive_interval(), Some(25));
    }

    #[test]
    fn test_decode_keys() {
        let cfg = make_valid_config();
        assert!(cfg.decode_private_key().is_ok());
        assert!(cfg.decode_peer_public_key().is_ok());
        assert!(cfg.decode_preshared_key().unwrap().is_none());
    }

    #[test]
    fn test_parse_peer_endpoint() {
        let cfg = make_valid_config();
        let addr = cfg.parse_peer_endpoint().unwrap();
        assert_eq!(addr.port(), 51820);
    }

    #[test]
    fn test_parse_address() {
        let cfg = make_valid_config();
        let (ip, prefix) = cfg.parse_address().unwrap();
        assert_eq!(ip, Ipv4Addr::new(10, 0, 0, 2));
        assert_eq!(prefix, 24);
    }

    #[test]
    fn test_deserialize_from_toml() {
        let toml_str = r#"
            enabled = true
            private_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            peer_public_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            peer_endpoint = "127.0.0.1:51820"
            address = "10.0.0.2/24"
        "#;
        let cfg: WireguardConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.persistent_keepalive, 25); // default
        assert_eq!(cfg.allowed_ips, vec!["10.0.0.0/24".to_string()]); // default
    }
}
