# Sockrats Architecture

## Overview

Sockrats is a Rust-based reverse tunneling client that connects to a [rathole](https://github.com/rapiz1/rathole) server and exposes an embedded SOCKS5 proxy and/or SSH server through the tunnel—without binding to any local network interface. The SOCKS5 and SSH servers operate entirely in-memory on tunnel streams.

### Key Features

- **Client-Only Mode**: No server-side logic; connects to a standard rathole server
- **Reverse SOCKS5 Tunneling**: Full SOCKS5 proxy (TCP CONNECT + UDP ASSOCIATE) running in-memory
- **Embedded SSH Server**: Feature-gated SSH server via `russh` with PTY support via `portable-pty`
- **Multi-Service Architecture**: Run multiple services (SOCKS5, SSH) simultaneously on different rathole service names
- **No Local Listeners**: All servers operate purely in-memory on tunnel data channel streams
- **Multiple Transport Options**: TCP, TLS (rustls), Noise protocol
- **Connection Pooling**: Pre-established data channel pool for improved performance
- **Cross-Platform**: Linux, macOS, Windows with static builds via Docker

## HARD MANDATORY Requirements

### 1. Test-Driven Development (TDD)

Every source file **MUST** include comprehensive unit tests. This is a **non-negotiable** requirement. All modules follow the pattern:

```rust
// ... module implementation above ...

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Test code here
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Async test code here
    }
}
```

**Test Coverage Requirements:**
- Minimum 80% code coverage enforced via `cargo tarpaulin --fail-under 80`
- All public functions must have at least one test
- Edge cases and error paths must be tested
- Feature-gated code uses `#[cfg(feature = "ssh")]` on tests that require the SSH feature

**Running Tests:**
```bash
# Run all tests with all features
cargo test --all-features --verbose

# Run tests in Docker
make test-docker

# Run coverage
make coverage
# or in Docker
make coverage-docker
```

### 2. Maximum 600 Lines Per File

Every source file **MUST** stay under 600 lines. This is enforced to maintain readability and modularity. Current file sizes demonstrate compliance:

| File                              | Lines | Purpose                      |
|-----------------------------------|-------|------------------------------|
| `src/ssh/handler.rs`              | ~500  | SSH Handler (largest file)   |
| `src/protocol/codec.rs`           | ~489  | Protocol codec               |
| `src/ssh/process.rs`              | ~480  | Shell/PTY process management |
| `src/ssh/config.rs`               | ~372  | SSH configuration            |
| `src/config/client.rs`            | ~355  | Client configuration         |
| `src/socks/auth/password.rs`      | ~348  | SOCKS5 password auth         |
| `src/socks/tcp_relay.rs`          | ~341  | TCP relay                    |
| `src/error.rs`                    | ~332  | Error types                  |
| `src/pool/tcp_pool.rs`            | ~319  | TCP channel pool             |
| `src/ssh/auth/authorized_keys.rs` | ~300  | Authorized keys parser       |
| `src/ssh/session.rs`              | ~292  | SSH session management       |

If a file approaches 600 lines, it must be split into submodules.

## Architecture Overview

### Multi-Service Architecture

```text
                                    ┌──────────────────────────────────┐
                                    │        Rathole Server            │
                                    │                                  │
                                    │  ┌─────────────┐ ┌────────────┐  │
 SOCKS5 Client ──► rathole:port1 ───┼──┤   socks5    │ │  ssh       ├──┼── SSH Client ──► rathole:port2
                                    │  │  service    │ │  service   │  │
                                    │  └──────┬──────┘ └─────┬──────┘  │
                                    │         │              │         │
                                    └─────────┼──────────────┼─────────┘
                                              │              │
                        ┌─────────────────────┼──────────────┼──────────────┐
                        │                     │  Sockrats    │              │
                        │                     ▼  Client      ▼              │
                        │  ┌─────────────────────────────────────────────┐  │
                        │  │              Control Channels               │  │
                        │  │  ┌─────────────────┐  ┌──────────────────┐  │  │
                        │  │  │ CC: "socks5"    │  │ CC: "ssh"        │  │  │
                        │  │  │ (SOCKS5 handler)│  │ (SSH handler)    │  │  │
                        │  │  └────────┬────────┘  └─────────┬────────┘  │  │
                        │  └───────────┼─────────────────────┼───────────┘  │
                        │              │                     │              │
                        │              ▼                     ▼              │
                        │  ┌─────────────────┐  ┌─────────────────────┐     │
                        │  │  Data Channel   │  │  Data Channel       │     │
                        │  │  → SOCKS5       │  │  → SSH Server       │     │
                        │  │    handler      │  │    (russh)          │     │
                        │  └─────────────────┘  └─────────────────────┘     │
                        └───────────────────────────────────────────────────┘
```

### Service-Specific Data Flow

**SOCKS5 Flow:**
```
Remote SOCKS5 Client → Rathole Server → Data Channel → handle_socks5_on_stream() → Target
```

**SSH Flow:**
```
Remote SSH Client → Rathole Server → Data Channel → handle_ssh_on_stream() → Shell/Exec/PTY
```

## Directory Structure

```
sockrats/
├── Cargo.toml                       # Package manifest with feature flags
├── Cargo.lock                       # Dependency lock file
├── Cross.toml                       # Cross-compilation configuration
├── Makefile                         # Build system with Docker cross-compilation
├── README.md                        # Project README
├── ARCHITECTURE.md                  # This file
├── .gitignore                       # Git ignore rules
│
├── src/
│   ├── main.rs                      # Entry point: CLI args, logging, signal handling
│   ├── lib.rs                       # Library root: module exports, VERSION, NAME constants
│   ├── error.rs                     # Error types: SockratsError, Socks5Error, Socks5ReplyCode
│   ├── helper.rs                    # Utilities: RetryConfig, copy_bidirectional, constants
│   │
│   ├── config/                      # Configuration module
│   │   ├── mod.rs                   # load_config(), parse_config()
│   │   ├── client.rs                # Config, ClientConfig, ServiceConfig, SocksConfig, ServiceType
│   │   ├── transport.rs             # TransportType, TransportConfig, TlsConfig, NoiseConfig, etc.
│   │   └── pool.rs                  # PoolConfig with validation
│   │
│   ├── protocol/                    # Rathole wire protocol
│   │   ├── mod.rs                   # Re-exports
│   │   ├── types.rs                 # Hello, Auth, Ack, ControlChannelCmd, DataChannelCmd, UdpTraffic
│   │   ├── codec.rs                 # read_*/write_* async codec functions, bincode serialization
│   │   └── digest.rs                # SHA-256 digest for authentication
│   │
│   ├── transport/                   # Transport layer abstraction
│   │   ├── mod.rs                   # Transport trait, TransportDyn/StreamDyn, create_transport()
│   │   ├── addr.rs                  # AddrMaybeCached with DNS caching
│   │   ├── tcp.rs                   # TcpTransport
│   │   ├── tls.rs                   # TlsTransport (rustls with NoVerifier for skip_verify)
│   │   └── noise.rs                 # NoiseTransport (snowstorm)
│   │
│   ├── client/                      # Client logic
│   │   ├── mod.rs                   # run_client() - transport selection entry point
│   │   ├── client.rs                # Client<T> - multi-service orchestration
│   │   ├── control_channel.rs       # ControlChannel<T> - handshake, reconnection, heartbeat
│   │   └── data_channel.rs          # ServiceHandler enum, run_data_channel() routing
│   │
│   ├── socks/                       # In-memory SOCKS5 server
│   │   ├── mod.rs                   # Re-exports all SOCKS5 components
│   │   ├── consts.rs                # SOCKS5 protocol constants
│   │   ├── types.rs                 # SocksCommand, TargetAddr enums
│   │   ├── handler.rs               # handle_socks5_on_stream() main entry
│   │   ├── tcp_relay.rs             # handle_tcp_connect(), relay_tcp()
│   │   ├── auth/                    # SOCKS5 authentication
│   │   │   ├── mod.rs               # AuthMethod enum, authenticate(), select_auth_method()
│   │   │   ├── none.rs              # NoAuth handler
│   │   │   └── password.rs          # PasswordAuth RFC 1929 implementation
│   │   ├── command/                 # SOCKS5 command handling
│   │   │   ├── mod.rs               # Re-exports
│   │   │   ├── parser.rs            # parse_command() with IPv4/IPv6/domain parsing
│   │   │   └── reply.rs             # build_reply(), send_success(), send_io_error(), etc.
│   │   └── udp/                     # UDP ASSOCIATE support
│   │       ├── mod.rs               # UdpRelay struct
│   │       ├── associate.rs         # handle_udp_associate() (virtual mode for reverse tunnel)
│   │       ├── forwarder.rs         # UdpForwarder with session management
│   │       └── packet.rs            # UdpPacket encode/decode per RFC 1928
│   │
│   ├── ssh/                         # Embedded SSH server (feature-gated: "ssh")
│   │   ├── mod.rs                   # handle_ssh_on_stream(), build_russh_config()
│   │   ├── config.rs                # SshConfig with validation
│   │   ├── handler.rs               # SshHandler implementing russh::server::Handler
│   │   ├── keys.rs                  # Host key management: load, generate, save, fingerprint
│   │   ├── session.rs               # SessionState, ChannelState, ChannelType
│   │   ├── process.rs               # ShellManager, ShellProcess, PtyConfig, exec_command()
│   │   └── auth/                    # SSH authentication
│   │       ├── mod.rs               # AuthResult enum
│   │       ├── authorized_keys.rs   # AuthorizedKeys parser (OpenSSH format)
│   │       ├── password.rs          # verify_password() with constant-time comparison
│   │       └── publickey.rs         # PublicKeyAuth, verify_public_key()
│   │
│   └── pool/                        # Connection pool
│       ├── mod.rs                   # ChannelType enum, create_pool()
│       ├── channel.rs               # PooledChannel<S> with metadata and staleness check
│       ├── guard.rs                 # PooledChannelGuard<S> RAII guard with mpsc return
│       ├── manager.rs               # PoolManager, PoolStats, PoolStatsSnapshot
│       └── tcp_pool.rs              # TcpChannelPool<T> - full pool with warm-up, maintenance
│
├── examples/                        # Example configurations
│   ├── config.toml                  # Full configuration with all options documented
│   ├── config-minimal.toml          # Minimal single-service configuration
│   └── config-multiple-minimal.toml # Minimal multi-service configuration
│
└── tests/                           # Integration tests
    ├── test-integration.sh          # Shell-based integration test script
    ├── common/
    │   └── mod.rs                   # Test utilities: mock streams, TestConfigBuilder, socks5_mock
    └── fixtures/
        ├── test-config.toml         # Multi-service test config
        ├── test-multi-service.toml  # Multi-service test config with global socks
        ├── test-socks5.toml         # SOCKS5-specific test config
        ├── test-ssh.toml            # SSH-specific test config
        └── rathole-server.toml      # Rathole server config for integration tests
```

## Build System

### Makefile

The project uses a `Makefile` with Docker-based cross-compilation targets:

```makefile
# Makefile targets
make build              # Build for current platform (cargo build --release)
make test               # Run all tests (cargo test --all-features --verbose)
make lint               # cargo fmt --check && cargo clippy
make fmt                # cargo fmt
make coverage           # cargo tarpaulin --out Html --fail-under 80
make clean              # Clean build artifacts and dist/

# Docker targets (for reproducible builds)
make build-docker       # Build in Docker (static with musl)
make test-docker        # Run tests in Docker
make coverage-docker    # Run coverage in Docker

# Cross-compilation (all produce static binaries)
make build-linux-docker         # Linux x86_64 + ARM64 (musl static)
make build-windows-docker       # Windows x86_64 (zigbuild)
make build-macos-docker         # macOS Intel + Apple Silicon (osxcross)
make build-all-docker           # All platforms
make release-archives           # Create tar.gz/zip archives in dist/release/
make targets                    # Show available targets
```

**Build images:**
- Linux/Windows: `rust:1.93.0-alpine3.23` (musl for static linking)
- macOS: `rust:slim-trixie` (Debian with osxcross for macOS SDK)

### Cross.toml

Configuration for the `cross` tool, defining per-target Docker images and pre-build steps:

```toml
[build.env]
passthrough = ["RUST_BACKTRACE", "RUST_LOG"]

[target.x86_64-unknown-linux-musl]
image = "ghcr.io/cross-rs/x86_64-unknown-linux-musl:main"
pre-build = ["apk add --no-cache openssl-dev openssl-libs-static"]

[target.aarch64-unknown-linux-musl]
image = "ghcr.io/cross-rs/aarch64-unknown-linux-musl:main"
pre-build = ["apk add --no-cache openssl-dev openssl-libs-static"]

[target.x86_64-pc-windows-gnu]
image = "ghcr.io/cross-rs/x86_64-pc-windows-gnu:main"
```

## Cargo.toml

```toml
[package]
name = "sockrats"
version = "0.1.0"
edition = "2021"
authors = ["Sockrats Contributors"]
description = "Reverse SOCKS5 tunneling client using rathole protocol"
license = "MIT"

[features]
default = ["rustls-tls", "noise", "ssh"]

# TLS support using rustls (pure Rust, easy static linking)
rustls-tls = ["tokio-rustls", "rustls-pemfile", "rustls-native-certs", "ring"]

# TLS support using native-tls (requires OpenSSL)
native-tls = ["tokio-native-tls"]
native-tls-vendored = ["tokio-native-tls", "openssl-vendored"]

# Vendored OpenSSL for static builds
openssl-vendored = ["openssl/vendored"]

# Noise protocol support
noise = ["snowstorm", "base64"]

# WebSocket support (config types exist, transport not yet implemented)
websocket-rustls = ["tokio-tungstenite", "tokio-util", "futures-core", "futures-sink", "rustls-tls"]
websocket-native-tls = ["tokio-tungstenite", "tokio-util", "futures-core", "futures-sink", "native-tls"]

# SSH server support
ssh = ["russh", "ssh-key", "rand", "portable-pty"]

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Core utilities
anyhow = "1.0"
thiserror = "1"
bytes = { version = "1", features = ["serde"] }
async-trait = "0.1"

# Configuration
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
clap = { version = "4.0", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Protocol serialization
bincode = "1"
sha2 = "0.10"
lazy_static = "1.4"

# Networking
socket2 = { version = "0.5", features = ["all"] }
backoff = { version = "0.4", features = ["tokio"] }

# SOCKS5 protocol
tokio-stream = "0.1"
futures = "0.3"

# TLS with rustls (default - pure Rust, easy static linking)
tokio-rustls = { version = "0.26", optional = true, default-features = false, features = ["ring", "tls12"] }
rustls-native-certs = { version = "0.8", optional = true }
rustls-pemfile = { version = "2.0", optional = true }
ring = { version = "0.17", optional = true }

# TLS with native-tls (optional)
tokio-native-tls = { version = "0.3", optional = true }
openssl = { version = "0.10", optional = true }

# Optional Noise
snowstorm = { version = "0.4", optional = true, features = ["stream"], default-features = false }
base64 = { version = "0.22", optional = true }

# Optional WebSocket
tokio-tungstenite = { version = "0.24", optional = true }
tokio-util = { version = "0.7", optional = true, features = ["io"] }
futures-core = { version = "0.3", optional = true }
futures-sink = { version = "0.3", optional = true }

# Optional SSH server (russh)
russh = { version = "0.57", optional = true }
ssh-key = { version = "0.6", optional = true, features = ["ed25519", "rsa", "std"] }
rand = { version = "0.8", optional = true }
portable-pty = { version = "0.8", optional = true }

# Proxy support for outbound connections
async-http-proxy = { version = "1.2", features = ["runtime-tokio", "basic-auth"] }
async-socks5 = "0.6"
url = { version = "2.2", features = ["serde"] }

[dev-dependencies]
tokio-test = "0.4"
env_logger = "0.11"
tempfile = "3"

[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
opt-level = "z"  # Optimize for size
```

## Module Details

### 1. Configuration (`src/config/`)

The configuration module is split across four files:

#### `src/config/mod.rs` — Loading

```rust
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let content = std::fs::read_to_string(path.as_ref())?;
    parse_config(&content)
}

pub fn parse_config(content: &str) -> Result<Config> {
    let config: Config = toml::from_str(content)?;
    Ok(config)
}
```

#### `src/config/client.rs` — Client & Service Configuration

The configuration supports two modes:

1. **Legacy single-service mode**: Uses top-level `service_name` and `token`
2. **Multi-service mode**: Uses `services: Vec<ServiceConfig>` array

```rust
/// Top-level configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    pub client: ClientConfig,
}

/// Service type discriminator
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Socks5,
    Ssh,
}

/// Individual service configuration for multi-service mode
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub token: String,
    #[serde(default)]
    pub service_type: ServiceType,
    #[serde(default)]
    pub socks: Option<SocksConfig>,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
}

/// Trait for querying service lists
pub trait ServiceListExt {
    fn get_service(&self, name: &str) -> Option<&ServiceConfig>;
    fn socks5_services(&self) -> Vec<&ServiceConfig>;
    fn ssh_services(&self) -> Vec<&ServiceConfig>;
}

/// Main client configuration
#[derive(Debug, Deserialize)]
pub struct ClientConfig {
    pub remote_addr: String,
    #[serde(default)]
    pub service_name: String,          // Legacy single-service
    #[serde(default)]
    pub token: String,                 // Legacy single-service
    #[serde(default)]
    pub transport: TransportConfig,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout: u64,
    #[serde(default)]
    pub socks: SocksConfig,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    #[serde(default)]
    pub pool: PoolConfig,
    #[serde(default)]
    pub services: Vec<ServiceConfig>,  // Multi-service mode
}
```

The `effective_services()` method unifies both modes:

```rust
impl ClientConfig {
    /// Returns the effective list of services.
    /// If multi-service mode (services array non-empty), returns those.
    /// Otherwise, creates a single ServiceConfig from legacy fields.
    pub fn effective_services(&self) -> Vec<ServiceConfig> {
        if !self.services.is_empty() {
            return self.services.clone();
        }
        // Fallback: create single service from legacy config
        vec![ServiceConfig {
            name: self.service_name.clone(),
            token: self.token.clone(),
            service_type: ServiceType::Socks5,
            socks: Some(self.socks.clone()),
            ssh: self.ssh.clone(),
        }]
    }
}
```

#### SOCKS5 Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SocksConfig {
    #[serde(default)]
    pub auth_required: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub allow_udp: bool,
    #[serde(default = "default_dns_resolve")]
    pub dns_resolve: bool,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
}
```

#### `src/config/transport.rs` — Transport Configuration

```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Tcp,
    Tls,
    Noise,
    Websocket,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransportConfig {
    #[serde(rename = "type", default)]
    pub transport_type: TransportType,
    #[serde(default)]
    pub tcp: TcpConfig,
    #[serde(default)]
    pub tls: TlsConfig,
    pub noise: Option<NoiseConfig>,
    #[serde(default)]
    pub websocket: WebsocketConfig,
}

pub struct TcpConfig {
    pub nodelay: bool,                // default: true
    pub keepalive_secs: u64,          // default: 20
    pub keepalive_interval: u64,      // default: 8
}

pub struct TlsConfig {
    pub hostname: Option<String>,
    pub trusted_root: Option<String>,
    pub skip_verify: bool,            // default: false
}

pub struct NoiseConfig {
    pub pattern: String,              // default: "Noise_NK_25519_ChaChaPoly_BLAKE2s"
    pub remote_public_key: String,
    pub local_private_key: Option<String>,
}
```

#### `src/config/pool.rs` — Pool Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    pub min_tcp_channels: usize,      // default: 2
    pub max_tcp_channels: usize,      // default: 10
    pub min_udp_channels: usize,      // default: 1
    pub max_udp_channels: usize,      // default: 5
    pub idle_timeout: u64,            // default: 300 (seconds)
    pub health_check_interval: u64,   // default: 30 (seconds)
    pub acquire_timeout: u64,         // default: 10 (seconds)
}

impl PoolConfig {
    pub fn validate(&self) -> Result<(), String> {
        // Validates min <= max for both TCP and UDP channels
        // Validates max > 0 for both
    }
}
```

### 2. Protocol Layer (`src/protocol/`)

Implements the rathole wire protocol for communication with the rathole server.

#### Types (`src/protocol/types.rs`)

```rust
pub type Digest = [u8; 32];
pub type ProtocolVersion = digest::Digest;

pub enum Hello {
    ControlChannelHello(ProtocolVersion, Digest),  // version, service_name_digest
    DataChannelHello(ProtocolVersion, Digest),      // version, session_key
}

pub struct Auth {
    pub digest: Digest,  // SHA-256(token + nonce)
}

pub enum Ack {
    Ok,
    ServiceNotExist,
    AuthFailed,
}

pub enum ControlChannelCmd {
    CreateDataChannel,
    HeartBeat,
}

pub enum DataChannelCmd {
    StartForwardTcp,
    StartForwardUdp,
}

pub struct UdpTraffic {
    pub from: SocketAddr,
    pub payload: Bytes,
}
```

#### Codec (`src/protocol/codec.rs`)

Provides async read/write functions for each protocol message type:

```rust
pub async fn read_hello<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Hello>;
pub async fn write_hello<T: AsyncWrite + Unpin>(conn: &mut T, hello: &Hello) -> Result<()>;
pub async fn read_auth<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Auth>;
pub async fn write_auth<T: AsyncWrite + Unpin>(conn: &mut T, auth: &Auth) -> Result<()>;
pub async fn read_ack<T: AsyncRead + AsyncWrite + Unpin>(conn: &mut T) -> Result<Ack>;
pub async fn write_ack<T: AsyncWrite + Unpin>(conn: &mut T, ack: &Ack) -> Result<()>;
pub async fn read_control_cmd<T>(conn: &mut T) -> Result<ControlChannelCmd>;
pub async fn write_control_cmd<T>(conn: &mut T, cmd: &ControlChannelCmd) -> Result<()>;
pub async fn read_data_cmd<T>(conn: &mut T) -> Result<DataChannelCmd>;
pub async fn write_data_cmd<T>(conn: &mut T, cmd: &DataChannelCmd) -> Result<()>;
```

All messages are serialized using `bincode` with a 2-byte big-endian length prefix.

#### Digest (`src/protocol/digest.rs`)

```rust
pub fn digest(data: &[u8]) -> Digest;  // SHA-256 hash
```

### 3. Transport Layer (`src/transport/`)

#### Transport Trait (`src/transport/mod.rs`)

```rust
pub struct SocketOpts {
    pub nodelay: bool,
    pub keepalive_secs: Option<u64>,
    pub keepalive_interval: Option<u64>,
}

impl SocketOpts {
    pub fn for_control_channel() -> Self;   // nodelay=true, keepalive=40s
    pub fn for_data_channel() -> Self;      // nodelay=true, keepalive=20s
    pub fn from_tcp_config(config: &TcpConfig) -> Self;
    pub fn apply(&self, stream: &TcpStream) -> std::io::Result<()>;
}

pub trait Transport: Debug + Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + Debug + 'static;

    fn new(config: &TransportConfig) -> Result<Self> where Self: Sized;
    fn hint(conn: &Self::Stream, opts: SocketOpts);
    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream>;
}
```

Dynamic dispatch traits for runtime transport selection:

```rust
pub trait TransportDyn: Debug + Send + Sync {
    async fn connect_dyn(&self, addr: &AddrMaybeCached) -> Result<Box<dyn StreamDyn>>;
}

pub trait StreamDyn: AsyncRead + AsyncWrite + Unpin + Send + Debug {}

pub fn create_transport(config: &TransportConfig) -> Result<Box<dyn TransportDyn>> {
    match config.transport_type {
        TransportType::Tcp => Ok(Box::new(TcpTransport::new(config)?)),
        TransportType::Tls => Ok(Box::new(TlsTransport::new(config)?)),
        TransportType::Noise => Ok(Box::new(NoiseTransport::new(config)?)),
        TransportType::Websocket => anyhow::bail!("WebSocket transport not yet implemented"),
    }
}
```

#### TCP Transport (`src/transport/tcp.rs`)

```rust
pub struct TcpTransport {
    socket_opts: SocketOpts,
    connect_timeout: Duration,
}

impl Transport for TcpTransport {
    type Stream = TcpStream;
    // Connects with timeout, applies socket options
}
```

#### TLS Transport (`src/transport/tls.rs`)

Uses `tokio-rustls` (pure Rust TLS). Supports `skip_verify` via a `NoVerifier` implementation:

```rust
pub struct TlsTransport {
    connector: TlsConnector,
    hostname: String,
    socket_opts: SocketOpts,
    connect_timeout: Duration,
}

impl Transport for TlsTransport {
    type Stream = TlsStream<TcpStream>;
    // Creates TLS connection over TCP with optional certificate verification
}
```

#### Noise Transport (`src/transport/noise.rs`)

Uses `snowstorm` crate for Noise protocol encryption:

```rust
pub struct NoiseTransport {
    builder: NoiseBuilder,
    socket_opts: SocketOpts,
    connect_timeout: Duration,
}

impl Transport for NoiseTransport {
    type Stream = NoiseStream<TcpStream>;
    // Creates Noise-encrypted connection over TCP
}
```

#### Address Caching (`src/transport/addr.rs`)

```rust
pub struct AddrMaybeCached {
    addr: String,
    cached: Option<SocketAddr>,
}

impl AddrMaybeCached {
    pub async fn resolve(&self) -> Result<SocketAddr>;       // Uses cache if available
    pub async fn resolve_fresh(&self) -> Result<SocketAddr>; // Always performs DNS lookup
    pub fn set_cached(&mut self, addr: SocketAddr);
    pub fn clear_cached(&mut self);
}
```

### 4. Client Logic (`src/client/`)

#### Entry Point (`src/client/mod.rs`)

```rust
pub async fn run_client(config: Config, shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
    let client_config = config.client;
    match client_config.transport.transport_type {
        TransportType::Tcp => {
            let transport = TcpTransport::new(&client_config.transport)?;
            Client::new(client_config).await?.run(shutdown_rx).await
        }
        TransportType::Tls => { /* Similar with TlsTransport */ }
        TransportType::Noise => { /* Similar with NoiseTransport */ }
        TransportType::Websocket => { anyhow::bail!("WebSocket transport not yet implemented") }
    }
}
```

#### Client (`src/client/client.rs`)

```rust
pub struct Client<T: Transport> {
    config: ClientConfig,
    _phantom: PhantomData<T>,
}

impl<T: Transport + 'static> Client<T> {
    pub async fn run(self, mut shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
        let services = self.config.effective_services();

        for service in &services {
            let service_config = self.create_service_config(service);
            // Spawn a control channel per service
            tokio::spawn(async move {
                let cc = ControlChannel::<T>::new(service_config);
                cc.run().await
            });
        }

        // Wait for shutdown signal
        shutdown_rx.recv().await?;
        Ok(())
    }

    fn create_service_config(&self, service: &ServiceConfig) -> ClientConfig {
        // Creates a ClientConfig with the service's name, token, and config
    }
}
```

#### Control Channel (`src/client/control_channel.rs`)

```rust
pub struct ControlChannel<T: Transport> {
    config: ClientConfig,
    _phantom: PhantomData<T>,
}

impl<T: Transport + 'static> ControlChannel<T> {
    pub async fn run(&self) -> Result<()> {
        let retry = RetryConfig::new(10); // Max 10 reconnection attempts
        let mut attempt = 0;

        loop {
            match self.run_once().await {
                Ok(_) => { attempt = 0; }
                Err(e) => {
                    attempt += 1;
                    if attempt >= retry.max_retries {
                        return Err(e);
                    }
                    let delay = retry.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn run_once(&self) -> Result<()> {
        // 1. Create transport and connect
        // 2. Perform handshake (Hello → Auth → Ack)
        // 3. Listen for ControlChannelCmd
    }

    async fn do_handshake<S>(&self, conn: &mut S) -> Result<Digest> {
        // Send Hello::ControlChannelHello with service name digest
        // Receive Hello from server (contains nonce)
        // Send Auth with SHA-256(token + nonce)
        // Receive Ack
        // Returns session_key for data channels
    }

    async fn handle_commands<S>(&self, conn: S, session_key: Digest) {
        // Loop reading ControlChannelCmd:
        //   CreateDataChannel → spawn run_data_channel()
        //   HeartBeat → respond with heartbeat
    }

    fn determine_service_handler(&self) -> ServiceHandler {
        // Returns Ssh if service name contains "ssh", otherwise Socks5
    }
}
```

#### Data Channel (`src/client/data_channel.rs`)

```rust
pub enum ServiceHandler {
    Socks5(SocksConfig),
    Ssh(Arc<SshConfig>),
}

pub async fn run_data_channel<T: Transport>(
    args: DataChannelArgs<T>,
) -> Result<()> {
    // 1. Connect to server, send DataChannelHello
    // 2. Read DataChannelCmd
    // 3. Route based on ServiceHandler:
    match cmd {
        DataChannelCmd::StartForwardTcp => {
            match handler {
                ServiceHandler::Socks5(config) => handle_socks5_on_stream(stream, &config).await,
                ServiceHandler::Ssh(config) => handle_ssh_on_stream(stream, config).await,
            }
        }
        DataChannelCmd::StartForwardUdp => {
            match handler {
                ServiceHandler::Socks5(config) => { /* UDP handling */ },
                ServiceHandler::Ssh(_) => { /* SSH doesn't use UDP */ },
            }
        }
    }
}
```

### 5. In-Memory SOCKS5 Handler (`src/socks/`)

The SOCKS5 server implements RFC 1928 (SOCKS5) and RFC 1929 (username/password auth).

#### Handler (`src/socks/handler.rs`)

```rust
pub async fn handle_socks5_on_stream<S>(mut stream: S, config: &SocksConfig) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // 1. Authentication negotiation
    let auth_method = authenticate(&mut stream, config).await?;

    // 2. Parse SOCKS5 command
    let (command, target_addr) = parse_command(&mut stream, config.dns_resolve).await?;

    // 3. Execute command
    match command {
        SocksCommand::TcpConnect => {
            handle_tcp_connect(&mut stream, target_addr, config.request_timeout).await
        }
        SocksCommand::UdpAssociate => {
            handle_udp_associate(stream, target_addr, config).await
        }
        _ => {
            send_command_not_supported(&mut stream).await
        }
    }
}

pub async fn handle_socks5_with_timeout<S>(
    stream: S, config: &SocksConfig, timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, handle_socks5_on_stream(stream, config)).await?
}
```

#### Authentication (`src/socks/auth/`)

```rust
pub enum AuthMethod {
    NoAuth,
    Password,
}

pub async fn authenticate<S>(stream: &mut S, config: &SocksConfig) -> Result<AuthMethod> {
    // 1. Read client's offered methods
    // 2. Select best method based on config
    // 3. Perform method-specific authentication
}

fn select_auth_method(methods: &[u8], config: &SocksConfig) -> Option<AuthMethod> {
    // If auth_required, requires Password method
    // Otherwise, prefers NoAuth
}
```

Password authentication (`src/socks/auth/password.rs`) implements RFC 1929:
```rust
impl PasswordAuth {
    pub async fn authenticate<S>(stream: &mut S, config: &SocksConfig) -> Result<()> {
        // Read: VER(1) | ULEN(1) | UNAME(1-255) | PLEN(1) | PASSWD(1-255)
        // Verify credentials against config
        // Send: VER(1) | STATUS(1)  (0x00 = success, 0x01 = failure)
    }
}
```

#### TCP Relay (`src/socks/tcp_relay.rs`)

```rust
pub async fn handle_tcp_connect<S>(
    stream: &mut S, target: TargetAddr, timeout: u64,
) -> Result<()> {
    // 1. Resolve target address
    // 2. Connect to target with timeout
    // 3. Send success reply with bind address
    // 4. Relay data bidirectionally
}

pub async fn relay_tcp<A, B>(a: A, b: B) -> Result<()>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    // tokio::io::copy_bidirectional for efficient data relay
}
```

#### UDP ASSOCIATE (`src/socks/udp/`)

UDP ASSOCIATE operates in "virtual mode" for reverse tunnel compatibility:

```rust
// src/socks/udp/associate.rs
pub async fn handle_udp_associate<S>(
    mut control_stream: S, _client_addr: TargetAddr, _config: &SocksConfig,
) -> Result<()> {
    // 1. Send success reply with virtual bind address (0.0.0.0:0)
    // 2. Monitor control stream for closure
    // The UDP association lives as long as the TCP control connection
}
```

UDP packet encoding (`src/socks/udp/packet.rs`):
```rust
pub struct UdpPacket {
    pub frag: u8,
    pub addr: TargetAddr,
    pub data: Bytes,
}

pub fn parse_udp_packet(data: &[u8]) -> Result<UdpPacket>;
pub fn encode_udp_packet(packet: &UdpPacket) -> Vec<u8>;
```

UDP forwarding (`src/socks/udp/forwarder.rs`):
```rust
pub struct UdpForwarder {
    sessions: Arc<RwLock<HashMap<SocketAddr, UdpSession>>>,
    outbound_tx: mpsc::Sender<UdpPacket>,
    session_timeout: Duration,  // default: 120s
}

impl UdpForwarder {
    pub async fn forward(&self, packet: UdpPacket) -> Result<()>;
    pub async fn cleanup_expired(&self);
}
```

#### Types (`src/socks/types.rs`)

```rust
pub enum SocksCommand {
    TcpConnect,    // 0x01
    TcpBind,       // 0x02
    UdpAssociate,  // 0x03
}

pub enum TargetAddr {
    Ip(SocketAddr),                    // IPv4 or IPv6
    Domain(String, u16),               // Domain name + port
}

impl TargetAddr {
    pub async fn resolve(&self) -> Result<SocketAddr>;
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn port(&self) -> u16;
    pub fn addr_type(&self) -> u8;
}
```

### 6. Embedded SSH Server (`src/ssh/`)

The SSH server is feature-gated behind `#[cfg(feature = "ssh")]`. When the feature is disabled, `handle_ssh_on_stream` returns an error.

#### Entry Point (`src/ssh/mod.rs`)

```rust
#[cfg(feature = "ssh")]
pub async fn handle_ssh_on_stream<S>(stream: S, config: Arc<SshConfig>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let russh_config = build_russh_config(&config)?;
    let pubkey_auth = PublicKeyAuth::from_config(&config)?;
    let handler = SshHandler::new(config, pubkey_auth);

    russh::server::run_stream(Arc::new(russh_config), stream, handler).await?;
    Ok(())
}

pub async fn handle_ssh_with_timeout<S>(
    stream: S, config: Arc<SshConfig>, timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, handle_ssh_on_stream(stream, config)).await?
}

fn build_russh_config(config: &SshConfig) -> Result<russh::server::Config> {
    // Load or generate host key
    // Set inactivity_timeout, auth_rejection_time, server_id
    // Configure keys = vec![host_key]
}
```

#### SSH Configuration (`src/ssh/config.rs`)

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SshConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auth_methods: Vec<String>,        // ["password", "publickey"]
    pub authorized_keys: Option<PathBuf>,
    pub host_key: Option<PathBuf>,
    pub password: Option<String>,
    pub username: Option<String>,
    pub server_id: Option<String>,
    #[serde(default = "default_true")]
    pub shell: bool,
    #[serde(default = "default_true")]
    pub exec: bool,
    #[serde(default)]
    pub sftp: bool,
    #[serde(default = "default_true")]
    pub pty: bool,
    #[serde(default)]
    pub tcp_forwarding: bool,
    #[serde(default)]
    pub x11_forwarding: bool,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default = "default_max_auth_tries")]
    pub max_auth_tries: u32,              // default: 6
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,          // default: 300 seconds
    #[serde(default = "default_shell")]
    pub default_shell: String,            // default: "/bin/sh" (Unix) or "cmd.exe" (Windows)
}

impl SshConfig {
    pub fn has_publickey_auth(&self) -> bool;
    pub fn has_password_auth(&self) -> bool;
    pub fn has_valid_auth(&self) -> bool;
    pub fn validate(&self) -> Result<(), String>;
}
```

#### SSH Handler (`src/ssh/handler.rs`)

Implements `russh::server::Handler`:

```rust
pub struct SshHandler {
    config: Arc<SshConfig>,
    pubkey_auth: Option<PublicKeyAuth>,
    session_state: SharedSessionState,
    shell_manager: ShellManager,
}

impl Handler for SshHandler {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth>;
    async fn auth_publickey_offered(&mut self, user: &str, key: &PublicKey) -> Result<Auth>;
    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth>;
    async fn channel_open_session(&mut self, channel: Channel<Msg>, session: &mut Session) -> Result<bool>;
    async fn pty_request(&mut self, channel_id: ChannelId, term: &str, col: u32, row: u32, pix_width: u32, pix_height: u32, modes: &[(Pty, u32)], session: &mut Session) -> Result<()>;
    async fn shell_request(&mut self, channel_id: ChannelId, session: &mut Session) -> Result<()>;
    async fn exec_request(&mut self, channel_id: ChannelId, data: &[u8], session: &mut Session) -> Result<()>;
    async fn env_request(&mut self, channel_id: ChannelId, name: &str, value: &str, session: &mut Session) -> Result<()>;
    async fn window_change_request(&mut self, channel_id: ChannelId, col: u32, row: u32, pix_width: u32, pix_height: u32, session: &mut Session) -> Result<()>;
    async fn channel_close(&mut self, channel_id: ChannelId, session: &mut Session) -> Result<()>;
    async fn data(&mut self, channel_id: ChannelId, data: &[u8], session: &mut Session) -> Result<()>;
    async fn subsystem_request(&mut self, channel_id: ChannelId, name: &str, session: &mut Session) -> Result<()>;
}
```

#### Session State (`src/ssh/session.rs`)

```rust
pub enum ChannelType {
    Session,
    DirectTcpip,
}

pub struct ChannelState {
    pub channel_type: ChannelType,
    pub has_pty: bool,
    pub term: Option<String>,
    pub cols: u32,
    pub rows: u32,
    pub pix_width: u32,
    pub pix_height: u32,
    pub env: HashMap<String, String>,
}

pub struct SessionState {
    pub authenticated: bool,
    pub username: Option<String>,
    pub auth_attempts: u32,
    pub max_auth_attempts: u32,
    pub channels: HashMap<u32, ChannelState>,
}

pub type SharedSessionState = Arc<Mutex<SessionState>>;
```

#### Process Management (`src/ssh/process.rs`)

```rust
pub struct ShellProcess {
    stdin_tx: mpsc::Sender<Vec<u8>>,
}

pub struct PtyConfig {
    pub term: String,         // default: "xterm-256color"
    pub cols: u16,            // default: 80
    pub rows: u16,            // default: 24
    pub pix_width: u16,       // default: 0
    pub pix_height: u16,      // default: 0
}

pub struct ShellManager {
    shells: Arc<Mutex<HashMap<u32, ShellProcess>>>,
}

impl ShellManager {
    pub async fn spawn_shell(
        &self, channel_id: u32, session: Handle, pty_config: Option<PtyConfig>,
        env: HashMap<String, String>, default_shell: &str,
    ) -> Result<()>;

    async fn spawn_shell_with_pty(/* ... */) -> Result<ShellProcess>;
    // Uses portable-pty crate for real PTY allocation

    async fn spawn_shell_no_pty(/* ... */) -> Result<ShellProcess>;
    // Falls back to tokio::process::Command with pipes

    pub async fn write_to_shell(&self, channel_id: u32, data: &[u8]) -> Result<bool>;
    pub async fn remove_shell(&self, channel_id: u32);
    pub async fn has_shell(&self, channel_id: u32) -> bool;
}

pub async fn exec_command(
    command: &str, session: Handle, channel_id: ChannelId,
    env: HashMap<String, String>,
) -> Result<()>;
// Executes a one-shot command and streams stdout/stderr back
```

#### Host Key Management (`src/ssh/keys.rs`)

```rust
pub fn load_host_key(path: &Path) -> Result<PrivateKey>;
pub fn generate_ed25519_key() -> Result<PrivateKey>;
pub fn save_host_key(key: &PrivateKey, path: &Path) -> Result<()>;
pub fn key_fingerprint(key: &PrivateKey) -> String;
```

When no `host_key` path is configured, `build_russh_config` generates an ephemeral Ed25519 key.

#### SSH Authentication (`src/ssh/auth/`)

```rust
// src/ssh/auth/mod.rs
pub enum AuthResult {
    Success,
    Failure,
    Partial,
}

// src/ssh/auth/password.rs
pub fn verify_password(config: &SshConfig, username: &str, password: &str) -> bool;
// Uses constant_time_compare() to prevent timing attacks

// src/ssh/auth/publickey.rs
pub struct PublicKeyAuth {
    authorized_keys: AuthorizedKeys,
}
pub fn verify_public_key(auth: Option<&PublicKeyAuth>, config: &SshConfig, key: &PublicKey) -> bool;

// src/ssh/auth/authorized_keys.rs
pub struct AuthorizedKey {
    pub key: PublicKey,
    pub comment: Option<String>,
    pub options: HashMap<String, Option<String>>,  // e.g., command="/bin/false", no-pty
}

pub struct AuthorizedKeys { keys: Vec<AuthorizedKey> }
impl AuthorizedKeys {
    pub fn from_file(path: &Path) -> Result<Self>;
    pub fn parse(content: &str) -> Result<Self>;
    pub fn is_authorized(&self, key: &PublicKey) -> bool;  // Compares SHA-256 fingerprints
    pub fn get_options(&self, key: &PublicKey) -> Option<&HashMap<String, Option<String>>>;
}
```

### 7. Connection Pool (`src/pool/`)

#### Pool Types (`src/pool/mod.rs`)

```rust
pub enum ChannelType {
    Tcp,
    Udp,
}

pub async fn create_pool<T: Transport + 'static>(/* ... */) -> Result<Arc<TcpChannelPool<T>>>;
```

#### Pooled Channel (`src/pool/channel.rs`)

```rust
pub struct PooledChannel<S> {
    stream: S,
    channel_type: ChannelType,
    created_at: Instant,
    last_used: Instant,
}

impl<S> PooledChannel<S> {
    pub fn new_tcp(stream: S) -> Self;
    pub fn new_udp(stream: S) -> Self;
    pub fn is_stale(&self, idle_timeout: Duration) -> bool;
    pub fn touch(&mut self);
    pub fn into_stream(self) -> S;
}
```

#### RAII Guard (`src/pool/guard.rs`)

```rust
pub struct PooledChannelGuard<S: Send + 'static> {
    stream: Option<S>,
    return_tx: mpsc::Sender<ReturnedChannel<S>>,
    is_tcp: bool,
}

pub struct ReturnedChannel<S> {
    pub stream: S,
    pub is_tcp: bool,
}

impl<S: Send + 'static> Drop for PooledChannelGuard<S> {
    fn drop(&mut self) {
        // Automatically returns the stream to the pool via mpsc channel
    }
}

impl<S: Send + 'static> Deref for PooledChannelGuard<S> { /* ... */ }
impl<S: Send + 'static> DerefMut for PooledChannelGuard<S> { /* ... */ }
```

#### TCP Channel Pool (`src/pool/tcp_pool.rs`)

```rust
pub struct TcpChannelPool<T: Transport> {
    config: PoolConfig,
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,
    channels: Mutex<VecDeque<PooledChannel<T::Stream>>>,
    create_semaphore: Semaphore,
    available_notify: Notify,
    active_count: AtomicUsize,
    manager: PoolManager,
    return_tx: mpsc::Sender<ReturnedChannel<T::Stream>>,
}

impl<T: Transport + 'static> TcpChannelPool<T> {
    pub async fn new(config, transport, remote_addr, session_key) -> Result<Arc<Self>>;
    // Creates pool, starts return handler, warms up, starts maintenance task

    async fn warm_up(self: &Arc<Self>) -> Result<()>;
    // Pre-creates min_tcp_channels connections

    async fn create_channel(&self) -> Result<()>;
    // Establishes a data channel: connect → Hello → read DataChannelCmd

    pub async fn acquire(&self) -> Result<PooledChannelGuard<T::Stream>>;
    // Gets channel from pool, removes stale, creates on-demand, waits with timeout

    async fn run_return_handler(self: Arc<Self>, rx: mpsc::Receiver<ReturnedChannel<T::Stream>>);
    // Receives returned channels and adds back to pool

    async fn run_maintenance(self: Arc<Self>);
    // Periodic: replenish to min channels, log health stats

    pub fn shutdown(&self);
    pub fn stats(&self) -> &PoolStats;
}
```

#### Pool Manager (`src/pool/manager.rs`)

```rust
pub struct PoolStats {
    // AtomicUsize counters: created, acquired, returned, expired, pooled
}

pub struct PoolStatsSnapshot {
    pub created: usize,
    pub acquired: usize,
    pub returned: usize,
    pub expired: usize,
    pub pooled: usize,
}

pub struct PoolManager {
    config: PoolConfig,
    stats: Arc<PoolStats>,
    shutdown: CancellationToken,  // or similar
}

impl PoolManager {
    pub fn new(config: PoolConfig, stats: Arc<PoolStats>) -> Self;
    pub fn stats(&self) -> &PoolStats;
    pub fn shutdown(&self);
    pub fn log_health(&self);
    pub fn health_check_interval(&self) -> Duration;
    pub async fn wait_shutdown(&self);
}
```

### 8. Error Types (`src/error.rs`)

```rust
#[derive(Debug, thiserror::Error)]
pub enum SockratsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SOCKS5 error: {0}")]
    Socks5(#[from] Socks5Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Configuration error: {0}")]
    Config(String),
    // ... more variants
}

#[derive(Debug, thiserror::Error)]
pub enum Socks5Error {
    #[error("Invalid SOCKS version: {0}")]
    InvalidVersion(u8),
    #[error("Unsupported command: {0}")]
    UnsupportedCommand(u8),
    #[error("Authentication failed")]
    AuthFailed,
    #[error("Connection refused")]
    ConnectionRefused,
    // ... more variants
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Socks5ReplyCode {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    ConnectionNotAllowed = 0x02,
    NetworkUnreachable = 0x03,
    HostUnreachable = 0x04,
    ConnectionRefused = 0x05,
    TtlExpired = 0x06,
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}
```

### 9. Helper Utilities (`src/helper.rs`)

```rust
pub const CHAN_SIZE: usize = 2048;
pub const TCP_POOL_SIZE: usize = 64;
pub const UDP_POOL_SIZE: usize = 64;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn copy_bidirectional<A, B>(a: &mut A, b: &mut B) -> io::Result<(u64, u64)>;

pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryConfig {
    pub fn new(max_retries: u32) -> Self;
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration;
    // Exponential backoff: min(base_delay * 2^attempt, max_delay)
}
```

### 10. Main Entry Point (`src/main.rs`)

```rust
#[derive(Parser, Debug)]
#[command(name = "sockrats")]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging format
    #[arg(long)]
    json_log: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    setup_logging(&args.log_level, args.json_log)?;

    let config = load_config(&args.config)?;

    // Shutdown signal handling (cross-platform)
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            // Handle both Ctrl+C and SIGTERM
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = signal(SignalKind::terminate()).recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        let _ = shutdown_tx.send(true);
    });

    run_client(config, shutdown_rx).await
}

fn setup_logging(level: &str, json: bool) -> Result<()> {
    // Supports JSON output via tracing-subscriber's json() formatter
    // Used for structured logging in production deployments
}
```

### 11. Library Root (`src/lib.rs`)

```rust
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod client;
pub mod config;
pub mod error;
pub mod helper;
pub mod pool;
pub mod protocol;
pub mod socks;
pub mod ssh;
pub mod transport;

pub use client::run_client;
pub use config::{load_config, Config};
pub use error::{Socks5Error, SockratsError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
```

## Example Configuration (`examples/config.toml`)

### Minimal Configuration

```toml
# examples/config-minimal.toml
[client]
remote_addr = "server.example.com:2333"
service_name = "socks5"
token = "your-secret-token"
```

### Full Configuration

```toml
# examples/config.toml
[client]
remote_addr = "server.example.com:2333"
service_name = "socks5"
token = "your-secret-token"
heartbeat_timeout = 40

[client.transport]
type = "tcp"

[client.transport.tcp]
nodelay = true
keepalive_secs = 20
keepalive_interval = 8

# TLS transport (uncomment to use)
# [client.transport.tls]
# hostname = "server.example.com"
# trusted_root = "/path/to/ca.crt"
# skip_verify = false

# Noise transport (uncomment to use)
# [client.transport.noise]
# pattern = "Noise_NK_25519_ChaChaPoly_BLAKE2s"
# remote_public_key = "base64-encoded-server-public-key"
# local_private_key = "base64-encoded-client-private-key"

[client.socks]
auth_required = false
# username = "user"
# password = "pass"
allow_udp = false
dns_resolve = true
request_timeout = 10

# SSH server (uncomment to enable)
# [client.ssh]
# enabled = true
# auth_methods = ["password", "publickey"]
# host_key = "/path/to/host_key"
# username = "admin"
# password = "secret"
# authorized_keys = "/path/to/authorized_keys"
# shell = true
# exec = true
# sftp = false
# pty = true
# tcp_forwarding = false
# max_auth_tries = 6
# connection_timeout = 300

[client.pool]
min_tcp_channels = 2
max_tcp_channels = 10
min_udp_channels = 1
max_udp_channels = 5
idle_timeout = 300
health_check_interval = 30
acquire_timeout = 10
```

### Multi-Service Configuration

```toml
# examples/config-multiple-minimal.toml
[client]
remote_addr = "rathole.example.com:2333"

# SOCKS5 service
[[client.services]]
name = "socks5"
token = "socks5-token"
service_type = "socks5"

# SSH service
[[client.services]]
name = "ssh"
token = "ssh-token"
service_type = "ssh"
ssh.password = "ssh-password"
ssh.authorized_keys = "/path/to/authorized_keys"
```

## Rathole Server Configuration

The rathole server must be configured with matching service names and tokens:

```toml
# rathole server.toml
[server]
bind_addr = "0.0.0.0:2333"

[server.services.socks5]
token = "socks5-token"
bind_addr = "0.0.0.0:1080"   # SOCKS5 clients connect here

[server.services.ssh]
token = "ssh-token"
bind_addr = "0.0.0.0:2222"   # SSH clients connect here
```

## Data Flow

### Connection Establishment

```text
1. Sockrats starts and reads config
2. For each service in effective_services():
   a. Creates a ControlChannel<T> with service-specific config
   b. ControlChannel connects to rathole server
   c. Sends Hello::ControlChannelHello with service_name digest
   d. Receives server Hello (contains nonce)
   e. Sends Auth with SHA-256(token + nonce)
   f. Receives Ack (Ok/AuthFailed/ServiceNotExist)
   g. Enters command loop: reads ControlChannelCmd
3. On CreateDataChannel:
   a. Spawns run_data_channel() task
   b. Connects new stream, sends DataChannelHello
   c. Reads DataChannelCmd (StartForwardTcp/StartForwardUdp)
   d. Routes to appropriate handler via ServiceHandler enum
```

### SOCKS5 Request Handling

```text
1. Data channel receives StartForwardTcp
2. ServiceHandler::Socks5 → handle_socks5_on_stream()
3. Authentication negotiation (NoAuth or Password per RFC 1929)
4. Parse SOCKS5 command (CONNECT or UDP ASSOCIATE)
5. For CONNECT:
   a. Resolve target address
   b. Establish TCP connection to target
   c. Send SOCKS5 success reply with bind address
   d. Relay data bidirectionally (tunnel ↔ target)
6. For UDP ASSOCIATE:
   a. Send success with virtual bind address (0.0.0.0:0)
   b. Monitor control stream for closure
```

### SSH Request Handling

```text
1. Data channel receives StartForwardTcp
2. ServiceHandler::Ssh → handle_ssh_on_stream()
3. build_russh_config() loads/generates host key
4. russh::server::run_stream() takes over the connection
5. SshHandler processes SSH protocol:
   a. Authentication (password or publickey)
   b. Channel open (session, direct-tcpip)
   c. PTY request → ShellManager::spawn_shell_with_pty() (portable-pty)
   d. Shell request → ShellManager::spawn_shell() (PTY or pipe fallback)
   e. Exec request → exec_command() (one-shot command)
   f. Data → write to shell stdin
   g. Shell stdout/stderr → send back to SSH client
```

### Service Handler Routing

The `determine_service_handler()` method in `ControlChannel` determines the handler:

```rust
fn determine_service_handler(&self) -> ServiceHandler {
    // If service name contains "ssh", use SSH handler
    // Otherwise, use SOCKS5 handler
    if self.config.service_name.to_lowercase().contains("ssh") {
        ServiceHandler::Ssh(Arc::new(self.config.ssh.clone().unwrap_or_default()))
    } else {
        ServiceHandler::Socks5(self.config.socks.clone())
    }
}
```

## Security Considerations

### General Security

- All transport options (TLS, Noise) provide encrypted communication
- Token-based authentication with nonce prevents replay attacks
- SHA-256 digest for service name and authentication
- Release builds strip symbols and use LTO

### SOCKS5 Security

- Optional username/password authentication (RFC 1929)
- DNS resolution can be configured client-side or passed through
- Connection timeouts prevent resource exhaustion

### SSH Security

- Password verification uses constant-time comparison (prevents timing attacks)
- Supports Ed25519 and RSA host keys
- Public key authentication via authorized_keys (OpenSSH format)
- Configurable max authentication attempts (default: 6)
- Connection timeout (default: 300s)
- Per-feature toggles: shell, exec, PTY, SFTP, TCP forwarding
- Ephemeral host key generation when no key file is configured

## Testing

### Unit Tests

Every module includes `#[cfg(test)] mod tests` with comprehensive coverage:

```bash
cargo test --all-features --verbose
```

### Integration Tests

Located in `tests/`:
- `tests/common/mod.rs` — Test utilities: `create_mock_stream_pair()`, `create_test_listener()`, `create_tcp_stream_pair()`, `TestConfigBuilder`, `socks5_mock` helpers
- `tests/fixtures/` — Test configuration files for various scenarios
- `tests/test-integration.sh` — Shell-based integration test script

### Test Fixtures

| File                                     | Purpose                                     |
|------------------------------------------|---------------------------------------------|
| `tests/fixtures/test-config.toml`        | Multi-service test (SOCKS5 + SSH)           |
| `tests/fixtures/test-multi-service.toml` | Multi-service with global socks config      |
| `tests/fixtures/test-socks5.toml`        | SOCKS5-specific test config                 |
| `tests/fixtures/test-ssh.toml`           | SSH-specific test config                    |
| `tests/fixtures/rathole-server.toml`     | Rathole server config for integration tests |

## Future Enhancements

- WebSocket transport implementation (config types already defined)
- SFTP subsystem support
- TCP/IP forwarding (direct-tcpip channels)
- X11 forwarding
- Agent forwarding
- UDP pool implementation (`UdpChannelPool`)
- Per-user SSH settings (authorized commands, forced commands)
- Metrics and observability endpoints
