//! Digest/hash functions for protocol
//!
//! Provides SHA-256 hashing used in the rathole protocol.

use super::types::{Digest, HASH_WIDTH_IN_BYTES};
use sha2::{Digest as Sha2Digest, Sha256};

/// Compute SHA-256 digest of data
///
/// This function computes a 32-byte SHA-256 hash of the input data.
/// It is used for service name hashing and authentication.
///
/// # Arguments
///
/// * `data` - The data to hash
///
/// # Returns
///
/// A 32-byte digest array
///
/// # Example
///
/// ```
/// use socksrat::protocol::digest;
///
/// let hash = digest(b"my-service-name");
/// assert_eq!(hash.len(), 32);
/// ```
pub fn digest(data: &[u8]) -> Digest {
    let d = Sha256::new().chain_update(data).finalize();
    let mut result = [0u8; HASH_WIDTH_IN_BYTES];
    result.copy_from_slice(&d);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_empty() {
        let hash = digest(b"");
        // SHA-256 of empty string is well-known
        let expected: [u8; 32] = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_digest_known_value() {
        // SHA-256("test") is well-known
        let hash = digest(b"test");
        let expected: [u8; 32] = [
            0x9f, 0x86, 0xd0, 0x81, 0x88, 0x4c, 0x7d, 0x65, 0x9a, 0x2f, 0xea, 0xa0, 0xc5, 0x5a,
            0xd0, 0x15, 0xa3, 0xbf, 0x4f, 0x1b, 0x2b, 0x0b, 0x82, 0x2c, 0xd1, 0x5d, 0x6c, 0x15,
            0xb0, 0xf0, 0x0a, 0x08,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_digest_deterministic() {
        let data = b"consistent input";
        let h1 = digest(data);
        let h2 = digest(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_digest_different_inputs_produce_different_hashes() {
        let h1 = digest(b"input1");
        let h2 = digest(b"input2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_digest_length() {
        let hash = digest(b"any data");
        assert_eq!(hash.len(), 32);
    }
}
