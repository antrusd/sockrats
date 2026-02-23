//! Authorized keys file parser
//!
//! Parses OpenSSH authorized_keys format files.

#[cfg(feature = "ssh")]
use anyhow::{Context, Result};
use std::collections::HashMap;
#[cfg(feature = "ssh")]
use std::path::Path;

#[cfg(feature = "ssh")]
use russh::keys::{HashAlg, PublicKey};

/// An entry in an authorized_keys file
#[derive(Debug, Clone)]
pub struct AuthorizedKey {
    /// The public key
    #[cfg(feature = "ssh")]
    pub key: PublicKey,
    /// Optional comment (usually username@host)
    pub comment: Option<String>,
    /// Key options (e.g., command=, no-pty, etc.)
    pub options: HashMap<String, Option<String>>,
}

/// Collection of authorized keys
#[derive(Debug, Default)]
pub struct AuthorizedKeys {
    #[cfg(feature = "ssh")]
    keys: Vec<AuthorizedKey>,
    #[cfg(not(feature = "ssh"))]
    keys: Vec<()>,
}

impl AuthorizedKeys {
    /// Create an empty authorized keys collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Load authorized keys from a file
    #[cfg(feature = "ssh")]
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read authorized_keys from {:?}", path))?;

        Self::parse(&content)
    }

    /// Parse authorized keys from a string
    #[cfg(feature = "ssh")]
    pub fn parse(content: &str) -> Result<Self> {
        let mut keys = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match parse_authorized_key_line(line) {
                Ok(key) => keys.push(key),
                Err(e) => {
                    tracing::warn!(line = line_num + 1, error = %e, "Skipping invalid key");
                }
            }
        }

        tracing::info!(count = keys.len(), "Loaded authorized keys");
        Ok(Self { keys })
    }

    /// Check if a public key is authorized
    #[cfg(feature = "ssh")]
    pub fn is_authorized(&self, key: &PublicKey) -> bool {
        // Compare fingerprints
        let target_fp = key.fingerprint(HashAlg::Sha256).to_string();

        self.keys.iter().any(|auth_key| {
            let auth_fp = auth_key.key.fingerprint(HashAlg::Sha256).to_string();
            auth_fp == target_fp
        })
    }

    /// Get the number of authorized keys
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Check if there are no authorized keys
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Get options for a key if it's authorized
    #[cfg(feature = "ssh")]
    pub fn get_options(&self, key: &PublicKey) -> Option<&HashMap<String, Option<String>>> {
        let target_fp = key.fingerprint(HashAlg::Sha256).to_string();

        self.keys.iter().find_map(|auth_key| {
            let auth_fp = auth_key.key.fingerprint(HashAlg::Sha256).to_string();
            if auth_fp == target_fp {
                Some(&auth_key.options)
            } else {
                None
            }
        })
    }
}

/// Parse a single line from authorized_keys file
#[cfg(feature = "ssh")]
fn parse_authorized_key_line(line: &str) -> Result<AuthorizedKey> {
    let mut options = HashMap::new();
    let mut remaining = line;

    // Check for options at the beginning
    // Options come before the key type and don't start with ssh-
    if !remaining.starts_with("ssh-")
        && !remaining.starts_with("ecdsa-")
        && !remaining.starts_with("sk-")
    {
        // Parse options until we find a key type
        let (opts, rest) = parse_options(remaining)?;
        options = opts;
        remaining = rest;
    }

    // Now parse the key itself using ssh-key crate
    // The format is: algorithm base64-key [comment]
    let parts: Vec<&str> = remaining.splitn(3, char::is_whitespace).collect();

    if parts.len() < 2 {
        anyhow::bail!("Invalid key format: missing algorithm or key data");
    }

    let key_str = if parts.len() >= 2 {
        format!("{} {}", parts[0], parts[1])
    } else {
        remaining.to_string()
    };

    let key = PublicKey::from_openssh(&key_str).context("Failed to parse public key")?;

    let comment = if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    };

    Ok(AuthorizedKey {
        key,
        comment,
        options,
    })
}

/// Parse key options from the beginning of a line
#[cfg(feature = "ssh")]
fn parse_options(line: &str) -> Result<(HashMap<String, Option<String>>, &str)> {
    let mut options = HashMap::new();
    let mut chars = line.char_indices().peekable();
    let mut start = 0;
    let mut in_quotes = false;

    while let Some((i, c)) = chars.next() {
        match c {
            '"' if !in_quotes => {
                in_quotes = true;
            }
            '"' if in_quotes => {
                in_quotes = false;
            }
            '\\' if in_quotes => {
                // Skip the next character (escaped)
                chars.next();
            }
            ' ' | '\t' if !in_quotes => {
                // End of options, parse the last one
                if i > start {
                    let opt = &line[start..i];
                    parse_single_option(opt, &mut options);
                }
                let remaining = &line[i..].trim_start();
                return Ok((options, remaining));
            }
            ',' if !in_quotes => {
                // Parse the option we just finished
                let opt = &line[start..i];
                parse_single_option(opt, &mut options);
                start = i + 1;
            }
            _ => {}
        }
    }

    // If we get here, the whole line was options (invalid)
    anyhow::bail!("No key found after options")
}

/// Parse a single option like "command=\"/bin/false\"" or "no-pty"
#[cfg(feature = "ssh")]
fn parse_single_option(opt: &str, options: &mut HashMap<String, Option<String>>) {
    if let Some(eq_pos) = opt.find('=') {
        let key = &opt[..eq_pos];
        let mut value = &opt[eq_pos + 1..];

        // Remove surrounding quotes
        if value.starts_with('"') && value.ends_with('"') {
            value = &value[1..value.len() - 1];
        }

        options.insert(key.to_string(), Some(value.to_string()));
    } else {
        options.insert(opt.to_string(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorized_keys_new() {
        let keys = AuthorizedKeys::new();
        assert!(keys.is_empty());
        assert_eq!(keys.len(), 0);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_simple_key() {
        let content = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl user@host";
        let keys = AuthorizedKeys::parse(content).unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_with_comment() {
        let content =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl my comment";
        let keys = AuthorizedKeys::parse(content).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys.keys[0].comment, Some("my comment".to_string()));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_multiple_keys() {
        let content = r#"
# This is a comment
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl user1@host

ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHu3t+bILVjHXPF9E0MnlSrk8FhRwplAqJqv8wnvmPjK user2@host
"#;
        let keys = AuthorizedKeys::parse(content).unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_skips_empty_lines_and_comments() {
        let content = r#"
# Comment line

   # Indented comment

ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl user@host

"#;
        let keys = AuthorizedKeys::parse(content).unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_single_option_flag() {
        let mut options = HashMap::new();
        parse_single_option("no-pty", &mut options);
        assert_eq!(options.get("no-pty"), Some(&None));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_parse_single_option_value() {
        let mut options = HashMap::new();
        parse_single_option("command=\"/bin/sh\"", &mut options);
        assert_eq!(options.get("command"), Some(&Some("/bin/sh".to_string())));
    }

    #[test]
    fn test_module_compiles_without_ssh_feature() {
        let keys = AuthorizedKeys::new();
        assert!(keys.is_empty());
    }
}
