//! SSH server configuration types
//!
//! This module defines configuration structures for the embedded SSH server.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// SSH server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Enable SSH server
    #[serde(default)]
    pub enabled: bool,

    /// Authentication methods (password, publickey)
    #[serde(default = "default_auth_methods")]
    pub auth_methods: Vec<String>,

    /// Path to authorized_keys file for public key authentication
    #[serde(default)]
    pub authorized_keys: Option<PathBuf>,

    /// Path to host key file (Ed25519 or RSA private key in OpenSSH format)
    #[serde(default)]
    pub host_key: Option<PathBuf>,

    /// Password for password authentication (if enabled)
    #[serde(default)]
    pub password: Option<String>,

    /// Username for password authentication (if enabled)
    #[serde(default)]
    pub username: Option<String>,

    /// Server identification string
    #[serde(default = "default_server_id")]
    pub server_id: String,

    /// Enable shell access
    #[serde(default = "default_true")]
    pub shell: bool,

    /// Enable exec command
    #[serde(default = "default_true")]
    pub exec: bool,

    /// Enable SFTP subsystem
    #[serde(default = "default_true")]
    pub sftp: bool,

    /// Path to sftp-server binary (for SFTP subsystem)
    #[serde(default = "default_sftp_server")]
    pub sftp_server: String,

    /// Enable PTY allocation
    #[serde(default = "default_true")]
    pub pty: bool,

    /// Enable TCP/IP forwarding
    #[serde(default)]
    pub tcp_forwarding: bool,

    /// Enable X11 forwarding
    #[serde(default)]
    pub x11_forwarding: bool,

    /// Enable agent forwarding
    #[serde(default)]
    pub agent_forwarding: bool,

    /// Maximum authentication attempts
    #[serde(default = "default_max_auth_tries")]
    pub max_auth_tries: u32,

    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,

    /// Default shell command
    #[serde(default = "default_shell")]
    pub default_shell: String,
}

fn default_auth_methods() -> Vec<String> {
    vec!["publickey".to_string(), "password".to_string()]
}

fn default_server_id() -> String {
    format!("SSH-2.0-Sockrats_{}", env!("CARGO_PKG_VERSION"))
}

fn default_true() -> bool {
    true
}

fn default_max_auth_tries() -> u32 {
    6
}

fn default_connection_timeout() -> u64 {
    300
}

fn default_shell() -> String {
    #[cfg(unix)]
    {
        "/bin/sh".to_string()
    }
    #[cfg(windows)]
    {
        "cmd.exe".to_string()
    }
    #[cfg(not(any(unix, windows)))]
    {
        "/bin/sh".to_string()
    }
}

fn default_sftp_server() -> String {
    // Common sftp-server paths on Linux/macOS
    for path in &[
        "/usr/lib/openssh/sftp-server",
        "/usr/libexec/openssh/sftp-server",
        "/usr/libexec/sftp-server",
        "/usr/lib/ssh/sftp-server",
    ] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    // Fallback: assume it's on PATH or use internal-sftp convention
    "/usr/lib/openssh/sftp-server".to_string()
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auth_methods: default_auth_methods(),
            authorized_keys: None,
            host_key: None,
            password: None,
            username: None,
            server_id: default_server_id(),
            shell: true,
            exec: true,
            sftp: true,
            sftp_server: default_sftp_server(),
            pty: true,
            tcp_forwarding: false,
            x11_forwarding: false,
            agent_forwarding: false,
            max_auth_tries: default_max_auth_tries(),
            connection_timeout: default_connection_timeout(),
            default_shell: default_shell(),
        }
    }
}

impl SshConfig {
    /// Create a new SSH configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if public key authentication is enabled
    pub fn has_publickey_auth(&self) -> bool {
        self.auth_methods.iter().any(|m| m == "publickey")
    }

    /// Check if password authentication is enabled
    pub fn has_password_auth(&self) -> bool {
        self.auth_methods.iter().any(|m| m == "password")
    }

    /// Check if any valid authentication method is configured
    pub fn has_valid_auth(&self) -> bool {
        // Password auth requires username and password
        let password_valid =
            self.has_password_auth() && self.username.is_some() && self.password.is_some();

        // Public key auth requires authorized_keys file
        let publickey_valid = self.has_publickey_auth() && self.authorized_keys.is_some();

        password_valid || publickey_valid
    }

    /// Validate the SSH configuration
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        // Must have at least one auth method
        if self.auth_methods.is_empty() {
            return Err("At least one authentication method must be specified".to_string());
        }

        // Validate auth methods
        for method in &self.auth_methods {
            if method != "password" && method != "publickey" {
                return Err(format!("Unknown authentication method: {}", method));
            }
        }

        // Password auth requires username and password
        if self.has_password_auth() {
            if self.username.is_none() {
                return Err("Username required for password authentication".to_string());
            }
            if self.password.is_none() {
                return Err("Password required for password authentication".to_string());
            }
        }

        // Public key auth requires authorized_keys
        if self.has_publickey_auth() && self.authorized_keys.is_none() {
            return Err("authorized_keys path required for public key authentication".to_string());
        }

        // Host key is required when enabled
        if self.host_key.is_none() {
            return Err("host_key path is required".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.auth_methods.len(), 2);
        assert!(config.has_publickey_auth());
        assert!(config.has_password_auth());
        assert!(config.shell);
        assert!(config.exec);
        assert!(config.sftp);
        assert!(!config.sftp_server.is_empty());
        assert!(config.pty);
        assert_eq!(config.max_auth_tries, 6);
        assert_eq!(config.connection_timeout, 300);
    }

    #[test]
    fn test_ssh_config_new() {
        let config = SshConfig::new();
        assert!(!config.enabled);
    }

    #[test]
    fn test_has_publickey_auth() {
        let mut config = SshConfig::default();
        assert!(config.has_publickey_auth());

        config.auth_methods = vec!["password".to_string()];
        assert!(!config.has_publickey_auth());
    }

    #[test]
    fn test_has_password_auth() {
        let mut config = SshConfig::default();
        assert!(config.has_password_auth());

        config.auth_methods = vec!["publickey".to_string()];
        assert!(!config.has_password_auth());
    }

    #[test]
    fn test_has_valid_auth_password() {
        let mut config = SshConfig::default();
        config.auth_methods = vec!["password".to_string()];
        assert!(!config.has_valid_auth());

        config.username = Some("user".to_string());
        assert!(!config.has_valid_auth());

        config.password = Some("pass".to_string());
        assert!(config.has_valid_auth());
    }

    #[test]
    fn test_has_valid_auth_publickey() {
        let mut config = SshConfig::default();
        config.auth_methods = vec!["publickey".to_string()];
        assert!(!config.has_valid_auth());

        config.authorized_keys = Some(PathBuf::from("/path/to/authorized_keys"));
        assert!(config.has_valid_auth());
    }

    #[test]
    fn test_validate_disabled() {
        let config = SshConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_no_auth_methods() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec![];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_auth_method() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["invalid".to_string()];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_password_missing_username() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["password".to_string()];
        config.password = Some("pass".to_string());
        config.host_key = Some(PathBuf::from("/path/to/host_key"));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_password_missing_password() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["password".to_string()];
        config.username = Some("user".to_string());
        config.host_key = Some(PathBuf::from("/path/to/host_key"));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_publickey_missing_authorized_keys() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["publickey".to_string()];
        config.host_key = Some(PathBuf::from("/path/to/host_key"));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_missing_host_key() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["password".to_string()];
        config.username = Some("user".to_string());
        config.password = Some("pass".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_password_config() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["password".to_string()];
        config.username = Some("user".to_string());
        config.password = Some("pass".to_string());
        config.host_key = Some(PathBuf::from("/path/to/host_key"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_valid_publickey_config() {
        let mut config = SshConfig::default();
        config.enabled = true;
        config.auth_methods = vec!["publickey".to_string()];
        config.authorized_keys = Some(PathBuf::from("/path/to/authorized_keys"));
        config.host_key = Some(PathBuf::from("/path/to/host_key"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_server_id_format() {
        let config = SshConfig::default();
        assert!(config.server_id.starts_with("SSH-2.0-Sockrats_"));
    }

    #[test]
    fn test_default_shell() {
        let shell = default_shell();
        #[cfg(unix)]
        assert_eq!(shell, "/bin/sh");
        #[cfg(windows)]
        assert_eq!(shell, "cmd.exe");
    }
}
