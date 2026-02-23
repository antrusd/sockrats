# Sockrats

A Rust-based reverse SOCKS5 tunneling client that connects to a rathole server and exposes a SOCKS5 proxy through the tunnel.

## Features

- **Client-Only Mode**: No server-side logic; connects to a standard rathole server
- **Reverse SOCKS Tunneling**: SOCKS5 traffic flows through the rathole tunnel
- **No Local Listeners**: SOCKS5 server operates purely in-memory on tunnel streams
- **Full UDP ASSOCIATE Support**: Complete UDP relay for DNS and other UDP protocols
- **Connection Pooling**: Pre-established data channel pool for improved performance
- **Encrypted Transport**: Noise protocol (pure Rust, zero C dependencies)
- **Cross-Platform**: Static builds for Linux, Windows, and macOS via zigbuild

## Quick Start

### Prerequisites

- Rust 1.70+ (or use Docker)
- A running rathole server

### Build

```bash
# Using cargo
cargo build --release

# Using Docker (Alpine, static musl)
make build

# Cross-compile all platforms
make build-all-docker
```

### Configure

Create a configuration file (see `examples/config.toml` for all options):

```toml
[client]
remote_addr = "server.example.com:2333"
service_name = "socks5"
token = "your-secret-token"
```

### Run

```bash
# Using cargo
cargo run -- -c config.toml

# Using the binary
./target/release/sockrats -c config.toml

# Using Docker
docker run -v ./config.toml:/app/config.toml sockrats -c /app/config.toml
```

## Rathole Server Configuration

On your rathole server, configure the service:

```toml
[server]
bind_addr = "0.0.0.0:2333"
default_token = "your-secret-token"

[server.services.socks5]
type = "tcp"
bind_addr = "0.0.0.0:1080"  # SOCKS5 clients connect here
```

## Usage

Once Sockrats is connected to the rathole server, SOCKS5 clients can connect to the server's bind address (e.g., `server:1080`):

```bash
# Test with curl
curl -x socks5://server.example.com:1080 https://example.com

# Use with any SOCKS5-aware application
```

## Development

### Run Tests

```bash
make test
# or
cargo test --all-features
```

### Check Code Coverage

```bash
make coverage
```

### Format and Lint

```bash
make fmt
make lint
```

## Cross-Platform Builds

All cross-compilation uses `cargo-zigbuild` via the `ghcr.io/rust-cross/cargo-zigbuild:0.21.4` Docker image. No OpenSSL or osxcross required — all dependencies are pure Rust.

### Supported Targets

| Platform       | Architecture      | Target Triple                  |
|----------------|-------------------|--------------------------------|
| Linux (static) | x86_64            | `x86_64-unknown-linux-musl`    |
| Linux (static) | ARM64             | `aarch64-unknown-linux-musl`   |
| Windows        | x86_64            | `x86_64-pc-windows-gnu`        |
| macOS          | x86_64 (Intel)    | `x86_64-apple-darwin`          |
| macOS          | ARM64 (M1/M2/M3) | `aarch64-apple-darwin`         |

### Building for Cross-Platform (Using Docker)

```bash
# Build for all Linux targets
make build-linux-docker

# Build for all Windows targets
make build-windows-docker

# Build for all macOS targets (Intel + Apple Silicon)
make build-macos-docker

# Build for all platforms
make build-all-docker

# Build for a specific target
make build-target-docker TARGET=x86_64-unknown-linux-musl

# Show all available targets
make targets
```

### Build Output

Binaries are placed in the `dist/` directory:
```
dist/
├── x86_64-unknown-linux-musl/sockrats
├── aarch64-unknown-linux-musl/sockrats
├── x86_64-pc-windows-gnu/sockrats.exe
├── x86_64-apple-darwin/sockrats        # Intel Mac
└── aarch64-apple-darwin/sockrats       # Apple Silicon M1/M2/M3
```

### Creating Release Archives

```bash
# Build all platforms and create archives
make build-all-docker
make release-archives
# Archives will be in dist/release/
```

### Using Pre-built Binaries

Pre-built binaries are available on the [Releases](https://github.com/antrusd/sockrats/releases) page for:

- **Linux**: `.tar.gz` archives for x86_64 and ARM64
- **Windows**: `.zip` archive for x86_64
- **macOS**: `.tar.gz` archives for x86_64 (Intel) and ARM64 (Apple Silicon)

## CI/CD

The GitHub Actions workflow automatically:

1. Runs tests and linting on every push/PR
2. Builds binaries for all supported platforms
3. Creates GitHub releases with pre-built binaries when tags are pushed
4. Pushes Docker images to Docker Hub on releases

### Creating a Release

```bash
# Tag a release
git tag v1.0.0
git push origin v1.0.0
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed design documentation.

## License

MIT License
