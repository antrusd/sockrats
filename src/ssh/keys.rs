//! SSH host key management
//!
//! This module handles loading and generating SSH host keys.

#[cfg(feature = "ssh")]
use anyhow::{Context, Result};
#[cfg(feature = "ssh")]
use std::path::Path;

#[cfg(feature = "ssh")]
use russh::keys::{HashAlg, PrivateKey};

/// Load a host key from a file
///
/// Supports Ed25519 and RSA keys in OpenSSH format.
#[cfg(feature = "ssh")]
pub fn load_host_key(path: &Path) -> Result<PrivateKey> {
    let key_data = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read host key from {:?}", path))?;

    parse_host_key(&key_data)
}

/// Parse a host key from PEM string
#[cfg(feature = "ssh")]
pub fn parse_host_key(pem_data: &str) -> Result<PrivateKey> {
    russh::keys::decode_secret_key(pem_data, None).context("Failed to parse private key")
}

/// Generate a new Ed25519 host key
#[cfg(feature = "ssh")]
pub fn generate_ed25519_key() -> Result<PrivateKey> {
    use rand::rngs::OsRng;
    use russh::keys::ssh_key::private::Ed25519Keypair;

    let keypair = Ed25519Keypair::random(&mut OsRng);
    let private_key = PrivateKey::from(keypair);
    Ok(private_key)
}

/// Save a host key to a file in OpenSSH format
#[cfg(feature = "ssh")]
pub fn save_host_key(key: &PrivateKey, path: &Path) -> Result<()> {
    use russh::keys::ssh_key::LineEnding;
    use std::fs;

    let openssh_str = key
        .to_openssh(LineEnding::LF)
        .context("Failed to encode key to OpenSSH format")?;

    // Write with restricted permissions
    fs::write(path, openssh_str.as_bytes())
        .with_context(|| format!("Failed to write host key to {:?}", path))?;

    // Set permissions to 600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }

    tracing::info!(path = ?path, "Saved host key");
    Ok(())
}

/// Get the public key fingerprint (SHA256)
#[cfg(feature = "ssh")]
pub fn key_fingerprint(key: &PrivateKey) -> String {
    let public_key = key.public_key();
    public_key.fingerprint(HashAlg::Sha256).to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_compiles_without_ssh_feature() {
        // This test ensures the module compiles without the ssh feature
        assert!(true);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_generate_ed25519_key() {
        use super::*;
        let key = generate_ed25519_key().unwrap();
        let fp = key_fingerprint(&key);
        assert!(fp.starts_with("SHA256:"));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_and_fingerprint() {
        use super::*;
        use russh::keys::ssh_key::LineEnding;

        // Generate a key and convert to OpenSSH format
        let key = generate_ed25519_key().unwrap();
        let openssh = key.to_openssh(LineEnding::LF).unwrap();

        // Parse it back
        let parsed = parse_host_key(&openssh).unwrap();

        // Fingerprints should match
        assert_eq!(key_fingerprint(&key), key_fingerprint(&parsed));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_save_and_load_key() {
        use super::*;

        let key = generate_ed25519_key().unwrap();

        // Create temp file
        let temp_dir = tempfile::tempdir().unwrap();
        let key_path = temp_dir.path().join("test_key");

        // Save
        save_host_key(&key, &key_path).unwrap();

        // Load
        let loaded = load_host_key(&key_path).unwrap();

        // Compare fingerprints
        assert_eq!(key_fingerprint(&key), key_fingerprint(&loaded));
    }
}
