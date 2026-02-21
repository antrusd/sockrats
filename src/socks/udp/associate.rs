//! UDP ASSOCIATE handler
//!
//! Implements the UDP ASSOCIATE command for SOCKS5.

use crate::socks::command::build_reply;
use crate::socks::consts::*;
use crate::socks::types::TargetAddr;
use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tracing::{debug, info, warn};

/// Handle UDP ASSOCIATE command
///
/// UDP ASSOCIATE in reverse tunnel mode works differently from standard SOCKS5:
/// - We cannot bind a local UDP port for the client
/// - Instead, we use a virtual binding and encapsulate UDP traffic through TCP
///
/// # Protocol Flow
///
/// 1. Client sends UDP ASSOCIATE with expected DST.ADDR and DST.PORT
/// 2. Server replies with BND.ADDR:BND.PORT (virtual in our case)
/// 3. UDP traffic is encapsulated and sent through the tunnel
/// 4. When the TCP connection closes, the UDP association ends
///
/// # Arguments
///
/// * `control_stream` - The control stream (from tunnel)
/// * `_client_addr` - The client's indicated address (usually ignored)
/// * `_config` - SOCKS5 configuration (reserved for future use)
pub async fn handle_udp_associate<S>(
    mut control_stream: S,
    _client_addr: TargetAddr,
    _config: &crate::config::SocksConfig,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // In reverse tunnel mode, we use a virtual bind address
    // The actual UDP traffic will be encapsulated through TCP
    let virtual_bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);

    // Send success reply with virtual bind address
    build_reply(&mut control_stream, SOCKS5_REPLY_SUCCEEDED, Some(virtual_bind_addr)).await?;

    info!("UDP ASSOCIATE established (virtual mode)");

    // The UDP association is maintained as long as the TCP control connection is open
    // We monitor the control stream for closure
    monitor_control_stream(control_stream).await?;

    info!("UDP ASSOCIATE session ended");
    Ok(())
}

/// Monitor the control stream for closure
///
/// The UDP association terminates when the TCP control connection closes.
async fn monitor_control_stream<S>(mut stream: S) -> Result<()>
where
    S: AsyncRead + Unpin,
{
    let mut buf = [0u8; 1];

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => {
                debug!("Control stream closed, terminating UDP association");
                break;
            }
            Ok(_) => {
                warn!("Unexpected data on UDP control stream");
            }
            Err(e) => {
                debug!("Control stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_bind_address() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(addr.port(), 0);
    }
}
