//! SOCKS5 command parsing module
//!
//! Handles parsing SOCKS5 commands and building replies.

mod parser;
mod reply;

pub use parser::parse_command;
pub use reply::build_reply;
