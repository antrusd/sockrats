//! VNC Remote Framebuffer (RFB) protocol constants and structures.
#![allow(dead_code)]
//!
//! This module provides the fundamental building blocks for VNC protocol communication,
//! including protocol version negotiation, message types, security handshakes, encodings,
//! and pixel format definitions. It implements the RFB protocol as specified in RFC 6143.

use bytes::{BufMut, BytesMut};

/// The RFB protocol version string advertised by the server.
pub const PROTOCOL_VERSION: &str = "RFB 003.008\n";

/// Maximum framebuffer update buffer size in bytes (32KB).
pub const UPDATE_BUF_SIZE: usize = 32768;

// --- Client-to-Server Message Types ---

/// Message type: Client requests to change the pixel format.
pub const CLIENT_MSG_SET_PIXEL_FORMAT: u8 = 0;

/// Message type: Client specifies supported encodings.
pub const CLIENT_MSG_SET_ENCODINGS: u8 = 2;

/// Message type: Client requests a framebuffer update.
pub const CLIENT_MSG_FRAMEBUFFER_UPDATE_REQUEST: u8 = 3;

/// Message type: Client sends a keyboard event.
pub const CLIENT_MSG_KEY_EVENT: u8 = 4;

/// Message type: Client sends a pointer (mouse) event.
pub const CLIENT_MSG_POINTER_EVENT: u8 = 5;

/// Message type: Client sends cut text (clipboard data).
pub const CLIENT_MSG_CLIENT_CUT_TEXT: u8 = 6;

// --- Server-to-Client Message Types ---

/// Message type: Server sends a framebuffer update.
pub const SERVER_MSG_FRAMEBUFFER_UPDATE: u8 = 0;

/// Message type: Server sends cut text (clipboard data).
pub const SERVER_MSG_SERVER_CUT_TEXT: u8 = 3;

// --- Encoding Types ---

/// Encoding type: Raw pixel data.
pub const ENCODING_RAW: i32 = 0;

/// Encoding type: Copy Rectangle.
pub const ENCODING_COPYRECT: i32 = 1;

/// Encoding type: Rise-and-Run-length Encoding.
pub const ENCODING_RRE: i32 = 2;

/// Encoding type: Compact RRE (CoRRE).
pub const ENCODING_CORRE: i32 = 4;

/// Encoding type: Hextile.
pub const ENCODING_HEXTILE: i32 = 5;

/// Encoding type: Zlib.
pub const ENCODING_ZLIB: i32 = 6;

/// Encoding type: Tight.
pub const ENCODING_TIGHT: i32 = 7;

/// Encoding type: ZlibHex.
pub const ENCODING_ZLIBHEX: i32 = 8;

/// Encoding type: ZRLE (Zlib Run-Length Encoding).
pub const ENCODING_ZRLE: i32 = 16;

/// Pseudo-encoding: JPEG Quality Level 0 (lowest quality).
pub const ENCODING_QUALITY_LEVEL_0: i32 = -32;

/// Pseudo-encoding: JPEG Quality Level 9 (highest quality).
pub const ENCODING_QUALITY_LEVEL_9: i32 = -23;

/// Pseudo-encoding: Compression Level 0 (no compression).
pub const ENCODING_COMPRESS_LEVEL_0: i32 = -256;

/// Pseudo-encoding: Compression Level 9 (maximum compression).
pub const ENCODING_COMPRESS_LEVEL_9: i32 = -247;

// --- Security Types ---

/// Security type: None (no authentication).
pub const SECURITY_TYPE_NONE: u8 = 1;

/// Security type: VNC Authentication.
pub const SECURITY_TYPE_VNC_AUTH: u8 = 2;

// --- Security Results ---

/// Security result: Authentication successful.
pub const SECURITY_RESULT_OK: u32 = 0;

/// Security result: Authentication failed.
pub const SECURITY_RESULT_FAILED: u32 = 1;

// --- Pixel Format ---

/// Describes the pixel format used for framebuffer data.
///
/// This matches the RFB protocol's PIXEL_FORMAT structure (16 bytes on wire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFormat {
    /// Bits per pixel (8, 16, or 32).
    pub bits_per_pixel: u8,
    /// Color depth (number of useful bits).
    pub depth: u8,
    /// Non-zero if multi-byte pixels are big-endian.
    pub big_endian_flag: u8,
    /// Non-zero if true color (as opposed to color map).
    pub true_colour_flag: u8,
    /// Maximum red value (2^n - 1).
    pub red_max: u16,
    /// Maximum green value (2^n - 1).
    pub green_max: u16,
    /// Maximum blue value (2^n - 1).
    pub blue_max: u16,
    /// Number of bits to left-shift red value.
    pub red_shift: u8,
    /// Number of bits to left-shift green value.
    pub green_shift: u8,
    /// Number of bits to left-shift blue value.
    pub blue_shift: u8,
}

impl PixelFormat {
    /// Standard RGBA32 pixel format (server native format).
    pub fn rgba32() -> Self {
        Self {
            bits_per_pixel: 32,
            depth: 24,
            big_endian_flag: 0,
            true_colour_flag: 1,
            red_max: 255,
            green_max: 255,
            blue_max: 255,
            red_shift: 0,
            green_shift: 8,
            blue_shift: 16,
        }
    }

    /// Write the pixel format to a buffer (16 bytes as per RFB spec).
    pub fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u8(self.bits_per_pixel);
        buf.put_u8(self.depth);
        buf.put_u8(self.big_endian_flag);
        buf.put_u8(self.true_colour_flag);
        buf.put_u16(self.red_max);
        buf.put_u16(self.green_max);
        buf.put_u16(self.blue_max);
        buf.put_u8(self.red_shift);
        buf.put_u8(self.green_shift);
        buf.put_u8(self.blue_shift);
        buf.put_bytes(0, 3); // padding
    }

    /// Parse a pixel format from a buffer (reads 16 bytes).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the buffer is too short.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, std::io::Error> {
        if buf.len() < 16 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Pixel format requires 16 bytes",
            ));
        }
        Ok(Self {
            bits_per_pixel: buf[0],
            depth: buf[1],
            big_endian_flag: buf[2],
            true_colour_flag: buf[3],
            red_max: u16::from_be_bytes([buf[4], buf[5]]),
            green_max: u16::from_be_bytes([buf[6], buf[7]]),
            blue_max: u16::from_be_bytes([buf[8], buf[9]]),
            red_shift: buf[10],
            green_shift: buf[11],
            blue_shift: buf[12],
            // bytes 13-15 are padding
        })
    }

    /// Check if this format is compatible with RGBA32 (no translation needed).
    pub fn is_compatible_with_rgba32(&self) -> bool {
        let rgba32 = Self::rgba32();
        self.bits_per_pixel == rgba32.bits_per_pixel
            && self.depth == rgba32.depth
            && self.true_colour_flag == rgba32.true_colour_flag
            && self.red_max == rgba32.red_max
            && self.green_max == rgba32.green_max
            && self.blue_max == rgba32.blue_max
            && self.red_shift == rgba32.red_shift
            && self.green_shift == rgba32.green_shift
            && self.blue_shift == rgba32.blue_shift
    }

    /// Check if this pixel format is valid.
    pub fn is_valid(&self) -> bool {
        matches!(self.bits_per_pixel, 8 | 16 | 32)
            && self.depth <= self.bits_per_pixel
            && self.true_colour_flag <= 1
    }
}

// --- ServerInit Message ---

/// Represents the ServerInit message sent during VNC initialization.
#[derive(Debug, Clone)]
pub struct ServerInit {
    /// The width of the framebuffer in pixels.
    pub framebuffer_width: u16,
    /// The height of the framebuffer in pixels.
    pub framebuffer_height: u16,
    /// The pixel format used by the framebuffer.
    pub pixel_format: PixelFormat,
    /// The name of the desktop.
    pub name: String,
}

impl ServerInit {
    /// Serializes the ServerInit message into a byte buffer.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u16(self.framebuffer_width);
        buf.put_u16(self.framebuffer_height);
        self.pixel_format.write_to(buf);

        let name_bytes = self.name.as_bytes();
        buf.put_u32(name_bytes.len() as u32);
        buf.put_slice(name_bytes);
    }
}

// --- Rectangle Header ---

/// Represents a rectangle header in a framebuffer update message.
#[derive(Debug)]
pub struct Rectangle {
    /// X coordinate of the top-left corner.
    pub x: u16,
    /// Y coordinate of the top-left corner.
    pub y: u16,
    /// Width of the rectangle in pixels.
    pub width: u16,
    /// Height of the rectangle in pixels.
    pub height: u16,
    /// The encoding type used for this rectangle's pixel data.
    pub encoding: i32,
}

impl Rectangle {
    /// Writes the rectangle header to a byte buffer (12 bytes).
    pub fn write_header(&self, buf: &mut BytesMut) {
        buf.put_u16(self.x);
        buf.put_u16(self.y);
        buf.put_u16(self.width);
        buf.put_u16(self.height);
        buf.put_i32(self.encoding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION.len(), 12);
        assert!(PROTOCOL_VERSION.starts_with("RFB"));
        assert!(PROTOCOL_VERSION.ends_with('\n'));
    }

    #[test]
    fn test_pixel_format_rgba32() {
        let pf = PixelFormat::rgba32();
        assert_eq!(pf.bits_per_pixel, 32);
        assert_eq!(pf.depth, 24);
        assert_eq!(pf.big_endian_flag, 0);
        assert_eq!(pf.true_colour_flag, 1);
        assert_eq!(pf.red_max, 255);
        assert_eq!(pf.green_max, 255);
        assert_eq!(pf.blue_max, 255);
        assert!(pf.is_valid());
    }

    #[test]
    fn test_pixel_format_compatible_with_self() {
        let pf = PixelFormat::rgba32();
        assert!(pf.is_compatible_with_rgba32());
    }

    #[test]
    fn test_pixel_format_write_read_roundtrip() {
        let pf = PixelFormat::rgba32();
        let mut buf = BytesMut::new();
        pf.write_to(&mut buf);

        assert_eq!(buf.len(), 16);

        let parsed = PixelFormat::from_bytes(&buf).unwrap();
        assert_eq!(pf, parsed);
    }

    #[test]
    fn test_pixel_format_from_bytes_too_short() {
        let buf = [0u8; 10];
        let result = PixelFormat::from_bytes(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_pixel_format_is_valid() {
        let mut pf = PixelFormat::rgba32();
        assert!(pf.is_valid());

        pf.bits_per_pixel = 24; // invalid
        assert!(!pf.is_valid());

        pf.bits_per_pixel = 16;
        pf.depth = 16;
        assert!(pf.is_valid());

        pf.bits_per_pixel = 8;
        pf.depth = 8;
        assert!(pf.is_valid());
    }

    #[test]
    fn test_pixel_format_depth_exceeds_bpp() {
        let pf = PixelFormat {
            bits_per_pixel: 16,
            depth: 24,
            ..PixelFormat::rgba32()
        };
        assert!(!pf.is_valid());
    }

    #[test]
    fn test_server_init_write() {
        let init = ServerInit {
            framebuffer_width: 800,
            framebuffer_height: 600,
            pixel_format: PixelFormat::rgba32(),
            name: "Test".to_string(),
        };

        let mut buf = BytesMut::new();
        init.write_to(&mut buf);

        // 2 (width) + 2 (height) + 16 (pixel format) + 4 (name length) + 4 (name "Test")
        assert_eq!(buf.len(), 2 + 2 + 16 + 4 + 4);

        // Verify width/height
        assert_eq!(u16::from_be_bytes([buf[0], buf[1]]), 800);
        assert_eq!(u16::from_be_bytes([buf[2], buf[3]]), 600);
    }

    #[test]
    fn test_rectangle_write_header() {
        let rect = Rectangle {
            x: 10,
            y: 20,
            width: 100,
            height: 200,
            encoding: ENCODING_RAW,
        };

        let mut buf = BytesMut::new();
        rect.write_header(&mut buf);

        assert_eq!(buf.len(), 12);
        assert_eq!(u16::from_be_bytes([buf[0], buf[1]]), 10);
        assert_eq!(u16::from_be_bytes([buf[2], buf[3]]), 20);
        assert_eq!(u16::from_be_bytes([buf[4], buf[5]]), 100);
        assert_eq!(u16::from_be_bytes([buf[6], buf[7]]), 200);
        assert_eq!(i32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]), 0);
    }

    #[test]
    fn test_encoding_constants() {
        assert_eq!(ENCODING_RAW, 0);
        assert_eq!(ENCODING_COPYRECT, 1);
        assert_eq!(ENCODING_RRE, 2);
        assert_eq!(ENCODING_HEXTILE, 5);
        assert_eq!(ENCODING_TIGHT, 7);
        assert_eq!(ENCODING_ZRLE, 16);
    }

    #[test]
    fn test_quality_level_range() {
        // Quality levels are -32 to -23 (10 levels)
        assert_eq!(ENCODING_QUALITY_LEVEL_9 - ENCODING_QUALITY_LEVEL_0, 9);
    }

    #[test]
    fn test_compression_level_range() {
        // Compression levels are -256 to -247 (10 levels)
        assert_eq!(ENCODING_COMPRESS_LEVEL_9 - ENCODING_COMPRESS_LEVEL_0, 9);
    }

    #[test]
    fn test_security_types() {
        assert_eq!(SECURITY_TYPE_NONE, 1);
        assert_eq!(SECURITY_TYPE_VNC_AUTH, 2);
    }

    #[test]
    fn test_security_results() {
        assert_eq!(SECURITY_RESULT_OK, 0);
        assert_eq!(SECURITY_RESULT_FAILED, 1);
    }

    #[test]
    fn test_pixel_format_not_compatible_different_shifts() {
        let mut pf = PixelFormat::rgba32();
        pf.red_shift = 16; // swap R and B shifts
        pf.blue_shift = 0;
        assert!(!pf.is_compatible_with_rgba32());
    }

    #[test]
    fn test_pixel_format_not_compatible_16bpp() {
        let pf = PixelFormat {
            bits_per_pixel: 16,
            depth: 16,
            big_endian_flag: 0,
            true_colour_flag: 1,
            red_max: 31,
            green_max: 63,
            blue_max: 31,
            red_shift: 11,
            green_shift: 5,
            blue_shift: 0,
        };
        assert!(!pf.is_compatible_with_rgba32());
        assert!(pf.is_valid());
    }
}
