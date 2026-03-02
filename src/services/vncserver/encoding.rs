//! Encoding dispatcher and pure Rust JPEG integration for VNC.
#![allow(dead_code)]
//!
//! This module wraps the `rfb-encodings` crate and adds a pure Rust JPEG encoder
//! (via `jpeg-encoder`) to replace TurboJPEG. It provides:
//!
//! - [`PureRustJpegEncoder`] — Encodes RGBA pixels to JPEG using `jpeg-encoder`
//! - [`TightZlibStreams`] — Persistent zlib streams implementing `TightStreamCompressor`
//! - Pixel format conversion between our [`PixelFormat`](super::protocol::PixelFormat)
//!   and [`rfb_encodings::PixelFormat`]
//! - Re-exports of key rfb-encodings functions

use bytes::{BufMut, BytesMut};
use flate2::{Compress, Compression, FlushCompress};

// Re-export key types and functions from rfb-encodings for use by client.rs
pub use rfb_encodings::tight::TightStreamCompressor;
pub use rfb_encodings::translate::translate_pixels;
pub use rfb_encodings::{encode_zlib_persistent, encode_zrle_persistent, get_encoder, Encoding};

use crate::services::vncserver::protocol::PixelFormat;

// --- Pixel Format Conversion ---

/// Convert our [`PixelFormat`] to [`rfb_encodings::PixelFormat`].
///
/// The two types have identical fields; this performs a field-by-field copy.
pub fn to_rfb_pixel_format(pf: &PixelFormat) -> rfb_encodings::PixelFormat {
    rfb_encodings::PixelFormat {
        bits_per_pixel: pf.bits_per_pixel,
        depth: pf.depth,
        big_endian_flag: pf.big_endian_flag,
        true_colour_flag: pf.true_colour_flag,
        red_max: pf.red_max,
        green_max: pf.green_max,
        blue_max: pf.blue_max,
        red_shift: pf.red_shift,
        green_shift: pf.green_shift,
        blue_shift: pf.blue_shift,
    }
}

/// Convert an [`rfb_encodings::PixelFormat`] back to our [`PixelFormat`].
pub fn from_rfb_pixel_format(pf: &rfb_encodings::PixelFormat) -> PixelFormat {
    PixelFormat {
        bits_per_pixel: pf.bits_per_pixel,
        depth: pf.depth,
        big_endian_flag: pf.big_endian_flag,
        true_colour_flag: pf.true_colour_flag,
        red_max: pf.red_max,
        green_max: pf.green_max,
        blue_max: pf.blue_max,
        red_shift: pf.red_shift,
        green_shift: pf.green_shift,
        blue_shift: pf.blue_shift,
    }
}

// --- Pure Rust JPEG Encoder ---

/// Tight JPEG control byte (0x09 << 4 = 0x90).
const TIGHT_JPEG_CONTROL: u8 = 0x90;

/// Pure Rust JPEG encoder that replaces TurboJPEG for VNC Tight encoding.
///
/// Uses the `jpeg-encoder` crate to encode RGBA pixel data into JPEG format,
/// producing the VNC Tight JPEG wire format (control byte + compact length + JPEG data).
pub struct PureRustJpegEncoder;

impl PureRustJpegEncoder {
    /// Encode RGBA pixel data to JPEG bytes.
    ///
    /// # Arguments
    ///
    /// * `pixels` - RGBA pixel data (4 bytes per pixel)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `quality` - JPEG quality (1-100)
    ///
    /// # Errors
    ///
    /// Returns an error string if encoding fails.
    pub fn encode_rgba(
        pixels: &[u8],
        width: u16,
        height: u16,
        quality: u8,
    ) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut output, quality);
        encoder
            .encode(pixels, width, height, jpeg_encoder::ColorType::Rgba)
            .map_err(|e| format!("JPEG encoding failed: {e}"))?;
        Ok(output)
    }

    /// Encode RGBA pixel data and produce the complete Tight JPEG wire format.
    ///
    /// The wire format is: `[0x90][compact_length][JPEG data]`
    ///
    /// # Arguments
    ///
    /// * `pixels` - RGBA pixel data (4 bytes per pixel)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `quality` - JPEG quality (1-100)
    ///
    /// # Errors
    ///
    /// Returns an error string if encoding fails.
    pub fn encode_tight_jpeg(
        pixels: &[u8],
        width: u16,
        height: u16,
        quality: u8,
    ) -> Result<BytesMut, String> {
        let jpeg_data = Self::encode_rgba(pixels, width, height, quality)?;

        let mut buf = BytesMut::with_capacity(1 + 3 + jpeg_data.len());
        buf.put_u8(TIGHT_JPEG_CONTROL);
        write_compact_length(&mut buf, jpeg_data.len());
        buf.put_slice(&jpeg_data);

        Ok(buf)
    }

    /// Map a VNC quality level (0-9) to a JPEG quality percentage (1-100).
    ///
    /// The mapping follows common VNC client conventions:
    /// - Level 0: quality 10 (lowest, fastest)
    /// - Level 5: quality 55 (balanced)
    /// - Level 9: quality 100 (highest, slowest)
    pub fn quality_level_to_jpeg_quality(level: u8) -> u8 {
        match level {
            0 => 10,
            1 => 15,
            2 => 25,
            3 => 37,
            4 => 50,
            5 => 55,
            6 => 65,
            7 => 78,
            8 => 90,
            _ => 100, // 9 and above
        }
    }
}

// --- Tight Zlib Streams ---

/// Manages persistent zlib compression streams for Tight encoding.
///
/// Per RFC 6143 Tight encoding specification, uses 4 separate zlib streams
/// to maintain compression dictionaries:
/// - Stream 0: Full-color (truecolor) data
/// - Stream 1: Mono rect (2-color bitmap) data
/// - Stream 2: Indexed palette (3-16 colors) data
/// - Stream 3: Reserved (unused)
///
/// Each stream preserves its dictionary state across multiple compress operations
/// for improved compression ratios.
pub struct TightZlibStreams {
    /// Array of 4 optional zlib compression streams.
    streams: [Option<Compress>; 4],
    /// Whether each stream has been initialized.
    active: [bool; 4],
    /// Compression level for each active stream.
    levels: [u8; 4],
}

impl TightZlibStreams {
    /// Creates a new [`TightZlibStreams`] with all streams uninitialized.
    pub fn new() -> Self {
        Self {
            streams: [None, None, None, None],
            active: [false; 4],
            levels: [0; 4],
        }
    }

    /// Gets or initializes a compression stream for the given stream ID.
    ///
    /// Lazy initialization: the stream is created on first use with the given
    /// compression level. Subsequent calls return the existing stream. If the
    /// compression level changes, the original level is preserved to avoid
    /// dictionary corruption (matching TigerVNC behavior).
    fn get_or_init_stream(&mut self, stream_id: usize, level: u8) -> &mut Compress {
        assert!(stream_id < 4, "stream_id must be 0-3");

        if !self.active[stream_id] {
            self.streams[stream_id] = Some(Compress::new(Compression::new(u32::from(level)), true));
            self.active[stream_id] = true;
            self.levels[stream_id] = level;
        }
        // Note: We intentionally do NOT recreate the stream on level change.
        // Recreating resets the dictionary, causing client decompression errors.
        // This matches TigerVNC behavior.

        self.streams[stream_id]
            .as_mut()
            .expect("stream just initialized")
    }

    /// Compress data using the specified stream.
    fn compress(&mut self, stream_id: usize, level: u8, input: &[u8]) -> Result<Vec<u8>, String> {
        let stream = self.get_or_init_stream(stream_id, level);

        // Allocate output buffer (compressed data + overhead)
        let mut output = vec![0u8; input.len() + 1024];
        let before_in = stream.total_in();
        let before_out = stream.total_out();

        match stream.compress(input, &mut output, FlushCompress::Sync) {
            Ok(_status) => {
                let bytes_consumed = (stream.total_in() - before_in) as usize;
                let bytes_produced = (stream.total_out() - before_out) as usize;

                if bytes_consumed != input.len() {
                    return Err(format!(
                        "Zlib only consumed {} of {} input bytes",
                        bytes_consumed,
                        input.len()
                    ));
                }

                output.truncate(bytes_produced);
                Ok(output)
            }
            Err(e) => Err(format!("Zlib compression failed: {e}")),
        }
    }
}

impl Default for TightZlibStreams {
    fn default() -> Self {
        Self::new()
    }
}

impl TightStreamCompressor for TightZlibStreams {
    fn compress_tight_stream(
        &mut self,
        stream_id: u8,
        level: u8,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.compress(stream_id as usize, level, input)
    }
}

// --- Compact Length Encoding ---

/// Write a compact length value per Tight encoding specification.
///
/// The encoding uses 1-3 bytes:
/// - Values < 128: 1 byte
/// - Values < 16384: 2 bytes (with continuation bits)
/// - Values < 4194304: 3 bytes (with continuation bits)
#[allow(clippy::cast_possible_truncation)]
fn write_compact_length(buf: &mut BytesMut, len: usize) {
    if len < 128 {
        buf.put_u8(len as u8);
    } else if len < 16384 {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8(((len >> 7) & 0x7F) as u8);
    } else {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8((((len >> 7) & 0x7F) | 0x80) as u8);
        buf.put_u8((len >> 14) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Pixel Format Conversion Tests ---

    #[test]
    fn test_to_rfb_pixel_format_rgba32() {
        let our_pf = PixelFormat::rgba32();
        let rfb_pf = to_rfb_pixel_format(&our_pf);

        assert_eq!(rfb_pf.bits_per_pixel, 32);
        assert_eq!(rfb_pf.depth, 24);
        assert_eq!(rfb_pf.big_endian_flag, 0);
        assert_eq!(rfb_pf.true_colour_flag, 1);
        assert_eq!(rfb_pf.red_max, 255);
        assert_eq!(rfb_pf.green_max, 255);
        assert_eq!(rfb_pf.blue_max, 255);
        assert_eq!(rfb_pf.red_shift, 0);
        assert_eq!(rfb_pf.green_shift, 8);
        assert_eq!(rfb_pf.blue_shift, 16);
    }

    #[test]
    fn test_from_rfb_pixel_format_roundtrip() {
        let our_pf = PixelFormat::rgba32();
        let rfb_pf = to_rfb_pixel_format(&our_pf);
        let back = from_rfb_pixel_format(&rfb_pf);
        assert_eq!(our_pf, back);
    }

    #[test]
    fn test_pixel_format_conversion_16bpp() {
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
        let rfb = to_rfb_pixel_format(&pf);
        let back = from_rfb_pixel_format(&rfb);
        assert_eq!(pf, back);
    }

    // --- PureRustJpegEncoder Tests ---

    #[test]
    fn test_encode_rgba_small_image() {
        // Create a 2x2 red image (RGBA)
        let pixels = vec![
            255, 0, 0, 255, // red
            255, 0, 0, 255, // red
            255, 0, 0, 255, // red
            255, 0, 0, 255, // red
        ];

        let result = PureRustJpegEncoder::encode_rgba(&pixels, 2, 2, 80);
        assert!(result.is_ok());
        let jpeg_data = result.unwrap();
        // JPEG files start with FF D8
        assert!(jpeg_data.len() >= 2);
        assert_eq!(jpeg_data[0], 0xFF);
        assert_eq!(jpeg_data[1], 0xD8);
    }

    #[test]
    fn test_encode_rgba_larger_image() {
        // 8x8 gradient image
        let mut pixels = Vec::with_capacity(8 * 8 * 4);
        for y in 0..8u8 {
            for x in 0..8u8 {
                pixels.push(x * 32); // R
                pixels.push(y * 32); // G
                pixels.push(128); // B
                pixels.push(255); // A
            }
        }

        let result = PureRustJpegEncoder::encode_rgba(&pixels, 8, 8, 50);
        assert!(result.is_ok());
    }

    #[test]
    fn test_encode_tight_jpeg_wire_format() {
        // 4x4 blue image
        let mut pixels = Vec::with_capacity(4 * 4 * 4);
        for _ in 0..16 {
            pixels.extend_from_slice(&[0, 0, 255, 255]); // RGBA blue
        }

        let result = PureRustJpegEncoder::encode_tight_jpeg(&pixels, 4, 4, 75);
        assert!(result.is_ok());

        let buf = result.unwrap();
        // First byte should be the Tight JPEG control byte
        assert_eq!(buf[0], TIGHT_JPEG_CONTROL);
        // Remaining bytes should contain the compact length + JPEG data
        assert!(buf.len() > 4); // control + at least 1 byte length + JPEG
    }

    #[test]
    fn test_quality_level_mapping() {
        assert_eq!(PureRustJpegEncoder::quality_level_to_jpeg_quality(0), 10);
        assert_eq!(PureRustJpegEncoder::quality_level_to_jpeg_quality(5), 55);
        assert_eq!(PureRustJpegEncoder::quality_level_to_jpeg_quality(9), 100);
        assert_eq!(PureRustJpegEncoder::quality_level_to_jpeg_quality(10), 100);
    }

    #[test]
    fn test_quality_levels_monotonically_increasing() {
        let mut prev = 0;
        for level in 0..=9 {
            let quality = PureRustJpegEncoder::quality_level_to_jpeg_quality(level);
            assert!(quality >= prev, "quality should increase with level");
            prev = quality;
        }
    }

    // --- Compact Length Tests ---

    #[test]
    fn test_compact_length_small() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 42);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0], 42);
    }

    #[test]
    fn test_compact_length_medium() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 200);
        assert_eq!(buf.len(), 2);
        // 200 = 0xC8 -> first byte = (200 & 0x7F) | 0x80 = 0xC8, second = 200 >> 7 = 1
        assert_eq!(buf[0], ((200 & 0x7F) | 0x80) as u8);
        assert_eq!(buf[1], (200 >> 7) as u8);
    }

    #[test]
    fn test_compact_length_large() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 20000);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_compact_length_boundary_127() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 127);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0], 127);
    }

    #[test]
    fn test_compact_length_boundary_128() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 128);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_compact_length_boundary_16383() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 16383);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_compact_length_boundary_16384() {
        let mut buf = BytesMut::new();
        write_compact_length(&mut buf, 16384);
        assert_eq!(buf.len(), 3);
    }

    // --- TightZlibStreams Tests ---

    #[test]
    fn test_tight_zlib_streams_new() {
        let streams = TightZlibStreams::new();
        assert!(!streams.active[0]);
        assert!(!streams.active[1]);
        assert!(!streams.active[2]);
        assert!(!streams.active[3]);
    }

    #[test]
    fn test_tight_zlib_streams_default() {
        let streams = TightZlibStreams::default();
        assert!(!streams.active[0]);
    }

    #[test]
    fn test_tight_zlib_streams_compress() {
        let mut streams = TightZlibStreams::new();

        let input = b"Hello, VNC encoding world! This is some test data for compression.";
        let result = streams.compress(0, 6, input);
        assert!(result.is_ok());

        let compressed = result.unwrap();
        assert!(!compressed.is_empty());
        // Stream should be active now
        assert!(streams.active[0]);
        assert_eq!(streams.levels[0], 6);
    }

    #[test]
    fn test_tight_zlib_streams_compress_multiple() {
        let mut streams = TightZlibStreams::new();

        // First compression
        let input1 = b"First block of data for zlib compression testing.";
        let result1 = streams.compress(0, 6, input1);
        assert!(result1.is_ok());

        // Second compression on same stream (should reuse dictionary)
        let input2 = b"Second block of similar data for zlib compression testing.";
        let result2 = streams.compress(0, 6, input2);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_tight_zlib_streams_different_streams() {
        let mut streams = TightZlibStreams::new();

        let input = b"Some data to compress";
        // Use different stream IDs
        assert!(streams.compress(0, 6, input).is_ok());
        assert!(streams.compress(1, 6, input).is_ok());
        assert!(streams.compress(2, 6, input).is_ok());

        assert!(streams.active[0]);
        assert!(streams.active[1]);
        assert!(streams.active[2]);
        assert!(!streams.active[3]);
    }

    #[test]
    #[should_panic(expected = "stream_id must be 0-3")]
    fn test_tight_zlib_streams_invalid_stream_id() {
        let mut streams = TightZlibStreams::new();
        let _ = streams.compress(4, 6, b"test");
    }

    #[test]
    fn test_tight_stream_compressor_trait() {
        let mut streams = TightZlibStreams::new();

        let input = b"Test data for TightStreamCompressor trait";
        let result = streams.compress_tight_stream(0, 6, input);
        assert!(result.is_ok());

        let compressed = result.unwrap();
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_tight_zlib_streams_level_change_preserves_stream() {
        let mut streams = TightZlibStreams::new();

        // Initialize with level 6
        let input = b"Initial data with level 6 compression";
        assert!(streams.compress(0, 6, input).is_ok());
        assert_eq!(streams.levels[0], 6);

        // Try to change to level 9 - should keep original level
        let input2 = b"More data with requested level 9";
        assert!(streams.compress(0, 9, input2).is_ok());
        // Level should remain 6 (original) to preserve dictionary
        assert_eq!(streams.levels[0], 6);
    }

    // --- Re-export Tests ---

    #[test]
    fn test_get_encoder_raw() {
        let encoder = get_encoder(rfb_encodings::ENCODING_RAW);
        assert!(encoder.is_some());
    }

    #[test]
    fn test_get_encoder_hextile() {
        let encoder = get_encoder(rfb_encodings::ENCODING_HEXTILE);
        assert!(encoder.is_some());
    }

    #[test]
    fn test_get_encoder_unknown() {
        let encoder = get_encoder(999);
        assert!(encoder.is_none());
    }
}
