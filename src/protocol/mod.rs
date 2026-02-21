//! Protocol module for SocksRat
//!
//! This module implements the rathole protocol for communication
//! with the rathole server. It must be compatible with the rathole
//! protocol implementation.

mod codec;
mod digest;
mod types;

pub use codec::{
    read_ack, read_auth, read_control_cmd, read_data_cmd, read_hello,
    write_ack, write_auth, write_control_cmd, write_data_cmd, write_hello,
};
pub use digest::digest;
pub use types::{
    Ack, Auth, ControlChannelCmd, DataChannelCmd, Digest, Hello,
    UdpTraffic, CURRENT_PROTO_VERSION, HASH_WIDTH_IN_BYTES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_produces_correct_length() {
        let data = b"test data";
        let result = digest(data);
        assert_eq!(result.len(), HASH_WIDTH_IN_BYTES);
    }

    #[test]
    fn test_digest_is_deterministic() {
        let data = b"same input";
        let d1 = digest(data);
        let d2 = digest(data);
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_digest_different_inputs() {
        let d1 = digest(b"input1");
        let d2 = digest(b"input2");
        assert_ne!(d1, d2);
    }
}
