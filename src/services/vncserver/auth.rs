//! VNC authentication implementation.
//!
//! This module implements VNC Authentication (security type 2) as specified in
//! RFC 6143 Section 7.2.2. It uses DES encryption with a VNC-specific bit reversal
//! quirk for challenge-response authentication.
//!
//! # Protocol
//!
//! The VNC authentication handshake works as follows:
//! 1. Server generates a 16-byte random challenge
//! 2. Server sends the challenge to the client
//! 3. Client encrypts the challenge using the password as the DES key (with bit-reversed bytes)
//! 4. Client sends the encrypted result back to the server
//! 5. Server verifies the response matches its own encryption of the challenge
//!
//! # Security Note
//!
//! VNC Authentication is a legacy protocol with known security limitations. It should
//! only be used on trusted networks or in conjunction with TLS/SSL tunneling (which
//! is the case when used via rathole tunnels in sockrats).

use des::cipher::{BlockEncrypt, KeyInit};
use des::Des;
use rand::Rng;

/// Handles VNC authentication using the VNC Authentication scheme (RFC 6143 §7.2.2).
///
/// Manages the server password, generates secure challenges for clients,
/// and verifies their DES-encrypted responses with the VNC-specific bit reversal quirk.
#[derive(Debug, Clone)]
pub struct VncAuth {
    /// The VNC password, if set. Only the first 8 bytes are used per the VNC spec.
    password: Option<String>,
}

impl VncAuth {
    /// Creates a new [`VncAuth`] instance.
    ///
    /// # Arguments
    ///
    /// * `password` - An optional VNC password. If `None`, authentication will always fail.
    ///   Only the first 8 characters of the password are significant per the VNC spec.
    pub fn new(password: Option<String>) -> Self {
        Self { password }
    }

    /// Returns whether a password is configured.
    #[allow(dead_code)]
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }

    /// Generates a cryptographically random 16-byte challenge for VNC authentication.
    ///
    /// This challenge is sent to the client, which must encrypt it with the shared
    /// secret (password) and return the result for verification.
    #[allow(clippy::unused_self)]
    pub fn generate_challenge(&self) -> [u8; 16] {
        let mut rng = rand::thread_rng();
        let mut challenge = [0u8; 16];
        rng.fill(&mut challenge);
        challenge
    }

    /// Verifies a client's authentication response against the challenge.
    ///
    /// The client's response should be the 16-byte challenge encrypted with the
    /// VNC password using DES-ECB with bit-reversed key bytes.
    ///
    /// # Arguments
    ///
    /// * `response` - The client's 16-byte encrypted response.
    /// * `challenge` - The original 16-byte challenge that was sent to the client.
    ///
    /// # Returns
    ///
    /// `true` if the response matches the expected encryption, `false` otherwise.
    /// Always returns `false` if no password is configured.
    pub fn verify_response(&self, response: &[u8], challenge: &[u8; 16]) -> bool {
        if response.len() != 16 {
            return false;
        }
        if let Some(ref password) = self.password {
            let expected = encrypt_challenge(challenge, password);
            // Constant-time comparison would be ideal, but VNC auth is already
            // cryptographically weak. The reference impl uses direct comparison.
            response == expected.as_slice()
        } else {
            false
        }
    }
}

/// Encrypts a 16-byte challenge with the VNC password using DES-ECB.
///
/// Implements the VNC-specific DES encryption where each password byte has
/// its bits reversed before being used as the DES key. The 16-byte challenge
/// is encrypted as two 8-byte DES blocks in ECB mode.
///
/// # Arguments
///
/// * `challenge` - A 16-byte array representing the challenge to encrypt.
/// * `password` - The VNC password string (only first 8 bytes are used).
///
/// # Returns
///
/// A `Vec<u8>` containing the 16-byte encrypted challenge.
fn encrypt_challenge(challenge: &[u8; 16], password: &str) -> Vec<u8> {
    // Prepare VNC password key (8 bytes, bit-reversed)
    let mut key = [0u8; 8];
    let pw_bytes = password.as_bytes();

    // Copy password bytes (up to 8), truncate or pad with zeros
    for (i, &byte) in pw_bytes.iter().take(8).enumerate() {
        key[i] = reverse_bits(byte);
    }

    // Create DES cipher with the VNC key
    let cipher = Des::new_from_slice(&key).expect("8-byte key should always be valid");

    // Encrypt the 16-byte challenge as two 8-byte blocks (DES ECB mode)
    let mut encrypted = vec![0u8; 16];

    // First 8-byte block
    let mut block1_bytes = [0u8; 8];
    block1_bytes.copy_from_slice(&challenge[0..8]);
    let mut block1 = block1_bytes.into();
    cipher.encrypt_block(&mut block1);
    encrypted[0..8].copy_from_slice(&block1);

    // Second 8-byte block
    let mut block2_bytes = [0u8; 8];
    block2_bytes.copy_from_slice(&challenge[8..16]);
    let mut block2 = block2_bytes.into();
    cipher.encrypt_block(&mut block2);
    encrypted[8..16].copy_from_slice(&block2);

    encrypted
}

/// Reverses the bits within a single byte.
///
/// This implements the historical VNC quirk where password bytes have their
/// bits reversed before being used as a DES key. For example:
///
/// `0b10110001` (177) becomes `0b10001101` (141).
fn reverse_bits(byte: u8) -> u8 {
    let mut result = 0u8;
    for i in 0..8 {
        if byte & (1 << i) != 0 {
            result |= 1 << (7 - i);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- reverse_bits tests ---

    #[test]
    fn test_reverse_bits_zero() {
        assert_eq!(reverse_bits(0x00), 0x00);
    }

    #[test]
    fn test_reverse_bits_all_ones() {
        assert_eq!(reverse_bits(0xFF), 0xFF);
    }

    #[test]
    fn test_reverse_bits_single_high_bit() {
        // 0b10000000 -> 0b00000001
        assert_eq!(reverse_bits(0x80), 0x01);
    }

    #[test]
    fn test_reverse_bits_single_low_bit() {
        // 0b00000001 -> 0b10000000
        assert_eq!(reverse_bits(0x01), 0x80);
    }

    #[test]
    fn test_reverse_bits_asymmetric() {
        // 0b10110001 (0xB1 = 177) -> 0b10001101 (0x8D = 141)
        assert_eq!(reverse_bits(0xB1), 0x8D);
    }

    #[test]
    fn test_reverse_bits_is_involution() {
        // Reversing bits twice should return the original value
        for byte in 0..=255u8 {
            assert_eq!(reverse_bits(reverse_bits(byte)), byte);
        }
    }

    // --- VncAuth construction tests ---

    #[test]
    fn test_vnc_auth_new_with_password() {
        let auth = VncAuth::new(Some("secret".to_string()));
        assert!(auth.has_password());
    }

    #[test]
    fn test_vnc_auth_new_without_password() {
        let auth = VncAuth::new(None);
        assert!(!auth.has_password());
    }

    #[test]
    fn test_vnc_auth_clone() {
        let auth = VncAuth::new(Some("test".to_string()));
        let cloned = auth.clone();
        assert!(cloned.has_password());
    }

    #[test]
    fn test_vnc_auth_debug() {
        let auth = VncAuth::new(Some("secret".to_string()));
        let debug = format!("{:?}", auth);
        assert!(debug.contains("VncAuth"));
    }

    // --- Challenge generation tests ---

    #[test]
    fn test_generate_challenge_returns_16_bytes() {
        let auth = VncAuth::new(Some("password".to_string()));
        let challenge = auth.generate_challenge();
        assert_eq!(challenge.len(), 16);
    }

    #[test]
    fn test_generate_challenge_is_random() {
        let auth = VncAuth::new(Some("password".to_string()));
        let challenge1 = auth.generate_challenge();
        let challenge2 = auth.generate_challenge();
        // Two random challenges should (almost certainly) be different
        assert_ne!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_challenge_works_without_password() {
        let auth = VncAuth::new(None);
        let challenge = auth.generate_challenge();
        assert_eq!(challenge.len(), 16);
    }

    // --- Verification tests ---

    #[test]
    fn test_verify_correct_password() {
        let password = "testpass";
        let auth = VncAuth::new(Some(password.to_string()));
        let challenge = auth.generate_challenge();

        // Simulate client encrypting the challenge with the same password
        let response = encrypt_challenge(&challenge, password);
        assert!(auth.verify_response(&response, &challenge));
    }

    #[test]
    fn test_verify_wrong_password() {
        let auth = VncAuth::new(Some("correct".to_string()));
        let challenge = auth.generate_challenge();

        // Client encrypts with wrong password
        let response = encrypt_challenge(&challenge, "wrong");
        assert!(!auth.verify_response(&response, &challenge));
    }

    #[test]
    fn test_verify_no_password_always_fails() {
        let auth = VncAuth::new(None);
        let challenge = auth.generate_challenge();

        // Even if the response is all zeros, it should fail
        let response = vec![0u8; 16];
        assert!(!auth.verify_response(&response, &challenge));
    }

    #[test]
    fn test_verify_empty_response_fails() {
        let auth = VncAuth::new(Some("password".to_string()));
        let challenge = auth.generate_challenge();
        assert!(!auth.verify_response(&[], &challenge));
    }

    #[test]
    fn test_verify_short_response_fails() {
        let auth = VncAuth::new(Some("password".to_string()));
        let challenge = auth.generate_challenge();
        assert!(!auth.verify_response(&[0u8; 8], &challenge));
    }

    #[test]
    fn test_verify_long_response_fails() {
        let auth = VncAuth::new(Some("password".to_string()));
        let challenge = auth.generate_challenge();
        assert!(!auth.verify_response(&[0u8; 32], &challenge));
    }

    // --- Password truncation tests ---

    #[test]
    fn test_password_truncated_to_8_chars() {
        // VNC only uses the first 8 characters of the password.
        // "longpassword" and "longpass" should produce the same result.
        let auth_long = VncAuth::new(Some("longpassword".to_string()));
        let auth_short = VncAuth::new(Some("longpass".to_string()));

        let challenge = [0x42u8; 16]; // Fixed challenge for deterministic test
        let response = encrypt_challenge(&challenge, "longpass");

        assert!(auth_short.verify_response(&response, &challenge));
        assert!(auth_long.verify_response(&response, &challenge));
    }

    #[test]
    fn test_short_password_padded_with_zeros() {
        // A 3-character password should work (padded to 8 bytes with zeros)
        let auth = VncAuth::new(Some("abc".to_string()));
        let challenge = [0x55u8; 16];
        let response = encrypt_challenge(&challenge, "abc");
        assert!(auth.verify_response(&response, &challenge));
    }

    #[test]
    fn test_empty_password_string() {
        // An empty string password is still a password (all zero key)
        let auth = VncAuth::new(Some(String::new()));
        let challenge = [0xAAu8; 16];
        let response = encrypt_challenge(&challenge, "");
        assert!(auth.verify_response(&response, &challenge));
    }

    // --- encrypt_challenge determinism test ---

    #[test]
    fn test_encrypt_challenge_deterministic() {
        let challenge = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let password = "test";

        let result1 = encrypt_challenge(&challenge, password);
        let result2 = encrypt_challenge(&challenge, password);
        assert_eq!(result1, result2);
        assert_eq!(result1.len(), 16);
    }

    #[test]
    fn test_encrypt_challenge_different_passwords_differ() {
        let challenge = [0xFFu8; 16];
        let result1 = encrypt_challenge(&challenge, "alpha");
        let result2 = encrypt_challenge(&challenge, "bravo");
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_encrypt_challenge_different_challenges_differ() {
        let challenge1 = [0x00u8; 16];
        let challenge2 = [0xFFu8; 16];
        let result1 = encrypt_challenge(&challenge1, "password");
        let result2 = encrypt_challenge(&challenge2, "password");
        assert_ne!(result1, result2);
    }
}
