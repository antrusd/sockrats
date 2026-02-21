//! SOCKS5 module for SocksRat
//!
//! This module implements the SOCKS5 protocol for handling proxy requests
//! through the rathole tunnel. It processes SOCKS5 requests directly on
//! the tunnel stream without binding to any local network interface.

mod auth;
mod command;
mod consts;
mod handler;
mod tcp_relay;
mod types;
mod udp;

pub use auth::{authenticate, AuthMethod};
pub use command::{parse_command, build_reply};
pub use consts::*;
pub use handler::handle_socks5_on_stream;
pub use tcp_relay::relay_tcp;
pub use types::{SocksCommand, TargetAddr};
pub use udp::{handle_udp_associate, UdpRelay};
