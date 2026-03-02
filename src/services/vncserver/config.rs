//! VNC server configuration types
//!
//! This module defines configuration structures for the embedded VNC server.

use serde::{Deserialize, Serialize};

/// Default framebuffer width
fn default_width() -> u16 {
    1024
}

/// Default framebuffer height
fn default_height() -> u16 {
    768
}

/// Default desktop name
fn default_desktop_name() -> String {
    "Sockrats VNC".to_string()
}

/// Default JPEG quality (0-100)
fn default_jpeg_quality() -> u8 {
    80
}

/// Default zlib compression level (0-9)
fn default_compression_level() -> u8 {
    6
}

/// Default maximum frames per second
fn default_max_fps() -> u8 {
    30
}

/// VNC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VncConfig {
    /// Enable VNC server
    #[serde(default)]
    pub enabled: bool,

    /// Framebuffer width in pixels
    #[serde(default = "default_width")]
    pub width: u16,

    /// Framebuffer height in pixels
    #[serde(default = "default_height")]
    pub height: u16,

    /// Desktop name advertised to VNC clients
    #[serde(default = "default_desktop_name")]
    pub desktop_name: String,

    /// Optional password for VNC authentication (None = no auth)
    #[serde(default)]
    pub password: Option<String>,

    /// JPEG quality level for Tight encoding (0-100)
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,

    /// Zlib compression level (0-9)
    #[serde(default = "default_compression_level")]
    pub compression_level: u8,

    /// Maximum frames per second
    #[serde(default = "default_max_fps")]
    pub max_fps: u8,
}

impl Default for VncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: default_width(),
            height: default_height(),
            desktop_name: default_desktop_name(),
            password: None,
            jpeg_quality: default_jpeg_quality(),
            compression_level: default_compression_level(),
            max_fps: default_max_fps(),
        }
    }
}

impl VncConfig {
    /// Create a new VNC configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the VNC configuration
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if self.width == 0 || self.height == 0 {
            return Err("Framebuffer dimensions must be greater than zero".to_string());
        }

        const MAX_DIMENSION: u16 = 8192;
        if self.width > MAX_DIMENSION || self.height > MAX_DIMENSION {
            return Err(format!(
                "Framebuffer dimensions too large: {}x{} (max: {})",
                self.width, self.height, MAX_DIMENSION
            ));
        }

        if self.jpeg_quality > 100 {
            return Err(format!(
                "JPEG quality must be 0-100, got: {}",
                self.jpeg_quality
            ));
        }

        if self.compression_level > 9 {
            return Err(format!(
                "Compression level must be 0-9, got: {}",
                self.compression_level
            ));
        }

        if self.max_fps == 0 {
            return Err("max_fps must be greater than zero".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vnc_config_default() {
        let config = VncConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.width, 1024);
        assert_eq!(config.height, 768);
        assert_eq!(config.desktop_name, "Sockrats VNC");
        assert!(config.password.is_none());
        assert_eq!(config.jpeg_quality, 80);
        assert_eq!(config.compression_level, 6);
        assert_eq!(config.max_fps, 30);
    }

    #[test]
    fn test_vnc_config_new() {
        let config = VncConfig::new();
        assert!(!config.enabled);
    }

    #[test]
    fn test_validate_disabled() {
        let config = VncConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_valid_config() {
        let config = VncConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_width() {
        let config = VncConfig {
            enabled: true,
            width: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_height() {
        let config = VncConfig {
            enabled: true,
            height: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_dimensions_too_large() {
        let config = VncConfig {
            enabled: true,
            width: 9000,
            height: 9000,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_max_dimensions() {
        let config = VncConfig {
            enabled: true,
            width: 8192,
            height: 8192,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_compression_level_too_high() {
        let config = VncConfig {
            enabled: true,
            compression_level: 10,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_fps() {
        let config = VncConfig {
            enabled: true,
            max_fps: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = VncConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            desktop_name: "Test Desktop".to_string(),
            password: Some("secret".to_string()),
            jpeg_quality: 90,
            compression_level: 3,
            max_fps: 60,
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: VncConfig = toml::from_str(&toml_str).unwrap();

        assert!(deserialized.enabled);
        assert_eq!(deserialized.width, 1920);
        assert_eq!(deserialized.height, 1080);
        assert_eq!(deserialized.desktop_name, "Test Desktop");
        assert_eq!(deserialized.password, Some("secret".to_string()));
        assert_eq!(deserialized.jpeg_quality, 90);
        assert_eq!(deserialized.compression_level, 3);
        assert_eq!(deserialized.max_fps, 60);
    }

    #[test]
    fn test_deserialize_minimal() {
        let toml_str = r#"
            enabled = true
        "#;
        let config: VncConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.width, 1024);
        assert_eq!(config.height, 768);
    }
}
