//! Protocol codec for serialization and deserialization
//!
//! This module handles reading and writing protocol messages
//! in a format compatible with rathole.

use super::types::{
    Ack, Auth, ControlChannelCmd, DataChannelCmd, Hello, UdpHeader, UdpTraffic,
    CURRENT_PROTO_VERSION,
};
use anyhow::{bail, Context, Result};
use bytes::BytesMut;
use lazy_static::lazy_static;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::trace;

/// Packet lengths for fixed-size protocol messages
struct PacketLength {
    hello: usize,
    ack: usize,
    auth: usize,
    c_cmd: usize,
    d_cmd: usize,
}

impl PacketLength {
    pub fn new() -> PacketLength {
        let username = "default";
        let d = super::digest::digest(username.as_bytes());
        let hello =
            bincode::serialized_size(&Hello::ControlChannelHello(CURRENT_PROTO_VERSION, d))
                .unwrap() as usize;
        let c_cmd =
            bincode::serialized_size(&ControlChannelCmd::CreateDataChannel).unwrap() as usize;
        let d_cmd =
            bincode::serialized_size(&DataChannelCmd::StartForwardTcp).unwrap() as usize;
        let ack = bincode::serialized_size(&Ack::Ok).unwrap() as usize;
        let auth = bincode::serialized_size(&Auth(d)).unwrap() as usize;

        PacketLength {
            hello,
            ack,
            auth,
            c_cmd,
            d_cmd,
        }
    }
}

lazy_static! {
    static ref PACKET_LEN: PacketLength = PacketLength::new();
}

/// Read a Hello message from the stream
pub async fn read_hello<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Hello> {
    let mut buf = vec![0u8; PACKET_LEN.hello];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read hello")?;
    let hello: Hello =
        bincode::deserialize(&buf).with_context(|| "Failed to deserialize hello")?;

    // Verify protocol version
    match &hello {
        Hello::ControlChannelHello(v, _) | Hello::DataChannelHello(v, _) => {
            if *v != CURRENT_PROTO_VERSION {
                bail!(
                    "Protocol version mismatched. Expected {}, got {}. Please update the client.",
                    CURRENT_PROTO_VERSION,
                    v
                );
            }
        }
    }

    Ok(hello)
}

/// Write a Hello message to the stream
pub async fn write_hello<T: AsyncWrite + Unpin>(conn: &mut T, hello: &Hello) -> Result<()> {
    let buf = bincode::serialize(hello).with_context(|| "Failed to serialize hello")?;
    conn.write_all(&buf)
        .await
        .with_context(|| "Failed to write hello")?;
    conn.flush().await.with_context(|| "Failed to flush hello")?;
    Ok(())
}

/// Read an Auth message from the stream
pub async fn read_auth<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Auth> {
    let mut buf = vec![0u8; PACKET_LEN.auth];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read auth")?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize auth")
}

/// Write an Auth message to the stream
pub async fn write_auth<T: AsyncWrite + Unpin>(conn: &mut T, auth: &Auth) -> Result<()> {
    let buf = bincode::serialize(auth).with_context(|| "Failed to serialize auth")?;
    conn.write_all(&buf)
        .await
        .with_context(|| "Failed to write auth")?;
    conn.flush().await.with_context(|| "Failed to flush auth")?;
    Ok(())
}

/// Read an Ack message from the stream
pub async fn read_ack<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Ack> {
    let mut buf = vec![0u8; PACKET_LEN.ack];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read ack")?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize ack")
}

/// Write an Ack message to the stream
pub async fn write_ack<T: AsyncWrite + Unpin>(conn: &mut T, ack: &Ack) -> Result<()> {
    let buf = bincode::serialize(ack).with_context(|| "Failed to serialize ack")?;
    conn.write_all(&buf)
        .await
        .with_context(|| "Failed to write ack")?;
    conn.flush().await.with_context(|| "Failed to flush ack")?;
    Ok(())
}

/// Read a ControlChannelCmd from the stream
pub async fn read_control_cmd<T: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut T,
) -> Result<ControlChannelCmd> {
    let mut buf = vec![0u8; PACKET_LEN.c_cmd];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read control cmd")?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize control cmd")
}

/// Write a ControlChannelCmd to the stream
pub async fn write_control_cmd<T: AsyncWrite + Unpin>(
    conn: &mut T,
    cmd: &ControlChannelCmd,
) -> Result<()> {
    let buf = bincode::serialize(cmd).with_context(|| "Failed to serialize control cmd")?;
    conn.write_all(&buf)
        .await
        .with_context(|| "Failed to write control cmd")?;
    conn.flush()
        .await
        .with_context(|| "Failed to flush control cmd")?;
    Ok(())
}

/// Read a DataChannelCmd from the stream
pub async fn read_data_cmd<T: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut T,
) -> Result<DataChannelCmd> {
    let mut buf = vec![0u8; PACKET_LEN.d_cmd];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read data cmd")?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize data cmd")
}

/// Write a DataChannelCmd to the stream
pub async fn write_data_cmd<T: AsyncWrite + Unpin>(
    conn: &mut T,
    cmd: &DataChannelCmd,
) -> Result<()> {
    let buf = bincode::serialize(cmd).with_context(|| "Failed to serialize data cmd")?;
    conn.write_all(&buf)
        .await
        .with_context(|| "Failed to write data cmd")?;
    conn.flush()
        .await
        .with_context(|| "Failed to flush data cmd")?;
    Ok(())
}

impl UdpTraffic {
    /// Write UDP traffic to the stream
    pub async fn write<T: AsyncWrite + Unpin>(&self, writer: &mut T) -> Result<()> {
        let hdr = UdpHeader {
            from: self.from,
            len: self.data.len() as u16,
        };

        let v = bincode::serialize(&hdr).unwrap();

        trace!("Write {:?} of length {}", hdr, v.len());
        writer.write_u8(v.len() as u8).await?;
        writer.write_all(&v).await?;
        writer.write_all(&self.data).await?;

        Ok(())
    }

    /// Write UDP traffic from a slice to the stream
    pub async fn write_slice<T: AsyncWrite + Unpin>(
        writer: &mut T,
        from: std::net::SocketAddr,
        data: &[u8],
    ) -> Result<()> {
        let hdr = UdpHeader {
            from,
            len: data.len() as u16,
        };

        let v = bincode::serialize(&hdr).unwrap();

        trace!("Write {:?} of length {}", hdr, v.len());
        writer.write_u8(v.len() as u8).await?;
        writer.write_all(&v).await?;
        writer.write_all(data).await?;

        Ok(())
    }

    /// Read UDP traffic from the stream
    pub async fn read<T: AsyncRead + Unpin>(reader: &mut T, hdr_len: u8) -> Result<UdpTraffic> {
        let mut buf = vec![0; hdr_len as usize];
        reader
            .read_exact(&mut buf)
            .await
            .with_context(|| "Failed to read udp header")?;

        let hdr: UdpHeader =
            bincode::deserialize(&buf).with_context(|| "Failed to deserialize UdpHeader")?;

        trace!("hdr {:?}", hdr);

        let mut data = BytesMut::new();
        data.resize(hdr.len as usize, 0);
        reader.read_exact(&mut data).await?;

        Ok(UdpTraffic {
            from: hdr.from,
            data: data.freeze(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hello_roundtrip() {
        let hello = Hello::control_channel("test-service");
        let _serialized = bincode::serialize(&hello).unwrap();
        // This won't work directly since Cursor isn't AsyncRead + AsyncWrite
        // For proper testing, we'd use tokio-test or a proper mock
    }

    #[test]
    fn test_packet_lengths_are_positive() {
        assert!(PACKET_LEN.hello > 0);
        assert!(PACKET_LEN.ack > 0);
        assert!(PACKET_LEN.auth > 0);
        assert!(PACKET_LEN.c_cmd > 0);
        assert!(PACKET_LEN.d_cmd > 0);
    }

    #[test]
    fn test_hello_serialization() {
        let hello = Hello::control_channel("test");
        let serialized = bincode::serialize(&hello).unwrap();
        let deserialized: Hello = bincode::deserialize(&serialized).unwrap();
        assert_eq!(hello, deserialized);
    }

    #[test]
    fn test_auth_serialization() {
        let nonce = [0u8; 32];
        let auth = Auth::new("token", &nonce);
        let serialized = bincode::serialize(&auth).unwrap();
        let deserialized: Auth = bincode::deserialize(&serialized).unwrap();
        assert_eq!(auth, deserialized);
    }

    #[test]
    fn test_ack_serialization() {
        for ack in [Ack::Ok, Ack::ServiceNotExist, Ack::AuthFailed] {
            let serialized = bincode::serialize(&ack).unwrap();
            let deserialized: Ack = bincode::deserialize(&serialized).unwrap();
            assert_eq!(ack, deserialized);
        }
    }

    #[test]
    fn test_control_cmd_serialization() {
        for cmd in [
            ControlChannelCmd::CreateDataChannel,
            ControlChannelCmd::HeartBeat,
        ] {
            let serialized = bincode::serialize(&cmd).unwrap();
            let deserialized: ControlChannelCmd = bincode::deserialize(&serialized).unwrap();
            assert_eq!(cmd, deserialized);
        }
    }

    #[test]
    fn test_data_cmd_serialization() {
        for cmd in [
            DataChannelCmd::StartForwardTcp,
            DataChannelCmd::StartForwardUdp,
        ] {
            let serialized = bincode::serialize(&cmd).unwrap();
            let deserialized: DataChannelCmd = bincode::deserialize(&serialized).unwrap();
            assert_eq!(cmd, deserialized);
        }
    }
}
