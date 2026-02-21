# SocksRat - Reverse Tunneling Client

## Overview

SocksRat is a Rust-based application that functions as a **reverse tunneling client** with embedded SOCKS5 and SSH servers. It connects to a remote rathole server and exposes services through that tunnel, without binding to any local network interface.

### Key Features

1. **Client-Only Mode**: No server-side logic; connects to a standard rathole server
2. **Reverse SOCKS Tunneling**: SOCKS5 traffic flows through the rathole tunnel
3. **Reverse SSH Server**: Full SSH server with shell/exec/SFTP capabilities via tunnel
4. **No Local Listeners**: All servers operate purely in-memory on tunnel streams
5. **Full UDP ASSOCIATE Support**: Complete UDP relay for DNS and other UDP protocols for embedded SOCKS5 server
6. **Connection Pooling**: Pre-established data channel pool for improved performance

---

## HARD MANDATORY Requirements

> ⚠️ **CRITICAL: These requirements MUST be followed strictly during implementation.**

### 1. Test-Driven Development (TDD)

All development MUST follow the TDD approach:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           TDD DEVELOPMENT CYCLE                             │
│                                                                             │
│    ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                  │
│    │   1. RED    │────►│  2. GREEN   │────►│ 3. REFACTOR │───┐              │
│    │ Write Test  │     │ Write Code  │     │ Clean Code  │   │              │
│    │ (must fail) │     │ (pass test) │     │ (keep pass) │   │              │
│    └─────────────┘     └─────────────┘     └─────────────┘   │              │
│          ▲                                                   │              │
│          └───────────────────────────────────────────────────┘              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**TDD Rules:**

1. **Write tests FIRST**: Before writing any production code, write a failing test
2. **Minimal implementation**: Write only enough code to make the test pass
3. **Refactor continuously**: Clean up code while keeping all tests green
4. **Test coverage**: Aim for >80% code coverage
5. **Test types required**:
   - Unit tests for all functions and methods
   - Integration tests for module interactions
   - End-to-end tests for full workflows

**Test File Structure:**
```rust
// Each module should have corresponding tests
// src/socks/handler.rs -> tests in src/socks/handler.rs (inline)
//                      -> or tests/socks_handler_test.rs (separate)

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests go here
    #[test]
    fn test_function_name_given_input_returns_expected() {
        // Arrange
        // Act
        // Assert
    }

    // Async tests
    #[tokio::test]
    async fn test_async_function_name() {
        // ...
    }
}
```

**Test Naming Convention:**
```
test_<function_name>_<scenario>_<expected_result>

Examples:
- test_parse_udp_header_valid_ipv4_returns_target_addr
- test_acquire_channel_pool_empty_creates_new_channel
- test_authenticate_invalid_password_returns_error
```

### 2. Maximum 600 Lines Per File

Every Rust source file MUST NOT exceed 600 lines of code.

**Rules:**

1. **Hard limit**: No file shall exceed 600 lines (including comments and whitespace)
2. **Split strategy**: When a file approaches 600 lines, split by logical responsibility
3. **Module organization**: Use Rust modules to organize split code
4. **Re-exports**: Use `mod.rs` to re-export from split files for clean API

**File Splitting Guidelines:**

```
When a file grows beyond 600 lines, consider splitting:

┌─────────────────────────────────────────────────────────────────┐
│ BEFORE (handler.rs - 800 lines)                                 │
├─────────────────────────────────────────────────────────────────┤
│ - Authentication logic (200 lines)                              │
│ - Command parsing (150 lines)                                   │
│ - TCP handling (200 lines)                                      │
│ - UDP handling (200 lines)                                      │
│ - Helper functions (50 lines)                                   │
└─────────────────────────────────────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ AFTER (split into multiple files)                               │
├─────────────────────────────────────────────────────────────────┤
│ socks/                                                          │
│ ├── mod.rs          (~50 lines)  - Module re-exports            │
│ ├── auth.rs         (~200 lines) - Authentication logic         │
│ ├── command.rs      (~150 lines) - Command parsing              │
│ ├── tcp_handler.rs  (~200 lines) - TCP handling                 │
│ ├── udp_handler.rs  (~200 lines) - UDP handling                 │
│ └── util.rs         (~50 lines)  - Helper functions             │
└─────────────────────────────────────────────────────────────────┘
```

**Module Re-export Pattern:**
```rust
// src/socks/mod.rs
mod auth;
mod command;
mod tcp_handler;
mod udp_handler;
mod util;

// Re-export public APIs
pub use auth::{authenticate, AuthMethod};
pub use command::{parse_command, SocksCommand};
pub use tcp_handler::handle_tcp_connect;
pub use udp_handler::handle_udp_associate;
pub(crate) use util::*;
```

**Line Counting:**
- Use `wc -l` or IDE line count
- Count all lines including:
  - Code
  - Comments
  - Documentation
  - Blank lines
  - Test modules (if inline)

**Enforcement:**
- CI/CD should fail builds if any file exceeds 600 lines
- Pre-commit hooks to warn developers
- Code review checklist item

---

## Architecture Diagram

### Multi-Service Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              REMOTE SIDE                                    │
│                                                                             │
│   ┌─────────────┐    ┌─────────────┐                                        │
│   │   Browser   │    │ SSH Client  │                                        │
│   │  (SOCKS5)   │    │ (OpenSSH)   │                                        │
│   └──────┬──────┘    └──────┬──────┘                                        │
│          │ :1080            │ :2222                                         │
│          │                  │                                               │
│   ┌──────▼──────────────────▼──────┐                                        │
│   │      Rathole Server            │                                        │
│   │  ┌──────────────────────────┐  │                                        │
│   │  │ socks5 service  :1080    │  │                                        │
│   │  │ ssh service     :2222    │  │                                        │
│   │  └────────────┬─────────────┘  │                                        │
│   └───────────────┼────────────────┘                                        │
│                   │                                                         │
│          Control + Data Channels                                            │
│                   │                                                         │
└───────────────────┼─────────────────────────────────────────────────────────┘
                    │
          ──────────┼────────── NAT/Firewall
                    │
┌───────────────────┼─────────────────────────────────────────────────────────┐
│                   │               LOCAL SIDE                                │
│                   │                                                         │
│   ┌───────────────▼────────────────────┐        ┌─────────────────────┐     │
│   │           SocksRat                 │        │  Local Network      │     │
│   │  ┌──────────────────────────────┐  │        │  Services           │     │
│   │  │     Control Channel          │  │        │                     │     │
│   │  │     (rathole protocol)       │  │        │  - Internal APIs    │     │
│   │  └─────────────┬────────────────┘  │        │  - Databases        │     │
│   │                │                   │        │  - Admin panels     │     │
│   │  ┌─────────────▼────────────────┐  │        │  - Any TCP/UDP      │     │
│   │  │   Data Channel Handler       │  │        │    endpoint         │     │
│   │  │   (service router)           │  │        └──────────▲──────────┘     │
│   │  └──────┬──────────────┬────────┘  │                   │                │
│   │         │              │           │                   │                │
│   │  ┌──────▼──────┐ ┌─────▼────────┐  │                   │                │
│   │  │ SOCKS5      │ │ SSH Server   │  │                   │                │
│   │  │ Handler     │ │ Handler      │  │                   │                │
│   │  │(fast-socks5)│ │(russh)       │  │                   │                │
│   │  │             │ │              │  │                   │                │
│   │  │- TCP CONNECT│ │- Shell/Exec  │  │                   │                │
│   │  │- UDP ASSOC  │ │- SFTP        │  │                   │                │
│   │  └──────┬──────┘ │- Port fwd    │  │                   │                │
│   │         │        └─────┬────────┘  │                   │                │
│   │         │              │           │                   │                │
│   │  ┌──────▼──────────────▼────────┐  │                   │                │
│   │  │   Outbound Connections       │──┼───────────────────┘                │
│   │  │   (to local network)         │  │                                    │
│   │  └──────────────────────────────┘  │                                    │
│   └────────────────────────────────────┘                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Service-Specific Data Flow

```
SOCKS5 Service:
  Remote Browser (SOCKS5 client)
       │
       ▼ connects to :1080
  Rathole Server
       │
       ▼ tunnel stream
  SocksRat SOCKS5 Handler
       │
       ▼ outbound TCP/UDP
  Local Network Target (e.g., internal-api.local:8080)


SSH Service:
  Remote SSH Client (ssh -p 2222 user@server)
       │
       ▼ connects to :2222
  Rathole Server
       │
       ▼ tunnel stream
  SocksRat SSH Handler (russh)
       │
       ├──▶ Shell session (bash, zsh, etc.)
       ├──▶ Exec command
       └──▶ SFTP file transfer
```

---

## Directory Structure

> 📏 **Note**: All files MUST stay under 600 lines. Estimated line counts shown below.

```
socksrat/
├── Cargo.toml
├── .github/
│   └── workflows/
│       └── ci.yml                    # CI with line count check
├── scripts/
│   └── check_line_count.sh           # Enforce 600 line limit
├── src/
│   ├── main.rs                       # (~80 lines)  Entry point, CLI parsing
│   ├── lib.rs                        # (~50 lines)  Library root, re-exports
│   │
│   ├── config/
│   │   ├── mod.rs                    # (~30 lines)  Config module root
│   │   ├── client.rs                 # (~150 lines) ClientConfig, SocksConfig
│   │   ├── transport.rs              # (~120 lines) TransportConfig types
│   │   └── pool.rs                   # (~80 lines)  PoolConfig
│   │
│   ├── protocol/
│   │   ├── mod.rs                    # (~40 lines)  Protocol module root
│   │   ├── types.rs                  # (~100 lines) Hello, Auth, Ack, Commands
│   │   ├── codec.rs                  # (~150 lines) Serialize/deserialize
│   │   └── digest.rs                 # (~50 lines)  SHA256 digest functions
│   │
│   ├── transport/
│   │   ├── mod.rs                    # (~100 lines) Transport trait, SocketOpts
│   │   ├── addr.rs                   # (~80 lines)  AddrMaybeCached
│   │   ├── tcp.rs                    # (~120 lines) TCP transport
│   │   ├── tls.rs                    # (~200 lines) TLS transport (optional)
│   │   ├── noise.rs                  # (~250 lines) Noise transport (optional)
│   │   └── websocket.rs              # (~300 lines) WebSocket transport (optional)
│   │
│   ├── client/
│   │   ├── mod.rs                    # (~50 lines)  Client module root
│   │   ├── client.rs                 # (~150 lines) Client struct, run()
│   │   ├── control_channel.rs        # (~250 lines) Control channel logic
│   │   └── data_channel.rs           # (~200 lines) Data channel handling
│   │
│   ├── socks/
│   │   ├── mod.rs                    # (~40 lines)  SOCKS5 module root
│   │   ├── consts.rs                 # (~50 lines)  SOCKS5 constants
│   │   ├── types.rs                  # (~100 lines) TargetAddr, Command enums
│   │   ├── auth/
│   │   │   ├── mod.rs                # (~30 lines)  Auth module root
│   │   │   ├── none.rs               # (~50 lines)  No authentication
│   │   │   └── password.rs           # (~120 lines) Username/password auth
│   │   ├── command/
│   │   │   ├── mod.rs                # (~30 lines)  Command module root
│   │   │   ├── parser.rs             # (~150 lines) Parse SOCKS5 commands
│   │   │   └── reply.rs              # (~100 lines) Build SOCKS5 replies
│   │   ├── handler.rs                # (~200 lines) Main SOCKS5 stream handler
│   │   ├── tcp_relay.rs              # (~180 lines) TCP CONNECT relay
│   │   └── udp/
│   │       ├── mod.rs                # (~30 lines)  UDP module root
│   │       ├── associate.rs          # (~250 lines) UDP ASSOCIATE handler
│   │       ├── packet.rs             # (~150 lines) UDP packet encode/decode
│   │       └── forwarder.rs          # (~200 lines) UDP session forwarder
│   │
│   ├── ssh/
│   │   ├── mod.rs                    # (~40 lines)  SSH module root
│   │   ├── config.rs                 # (~120 lines) SSH server configuration
│   │   ├── keys.rs                   # (~150 lines) Host key management
│   │   ├── auth/
│   │   │   ├── mod.rs                # (~30 lines)  Auth module root
│   │   │   ├── publickey.rs          # (~180 lines) Public key authentication
│   │   │   ├── password.rs           # (~120 lines) Password authentication
│   │   │   └── authorized_keys.rs    # (~150 lines) Authorized keys parser
│   │   ├── handler.rs                # (~250 lines) SSH Handler implementation
│   │   ├── session.rs                # (~200 lines) SSH session management
│   │   └── channel/
│   │       ├── mod.rs                # (~30 lines)  Channel module root
│   │       ├── session.rs            # (~200 lines) Session channel handler
│   │       └── exec.rs               # (~150 lines) Command execution handler
│   │
│   ├── pool/
│   │   ├── mod.rs                    # (~40 lines)  Pool module root
│   │   ├── channel.rs                # (~100 lines) PooledChannel struct
│   │   ├── tcp_pool.rs               # (~250 lines) TCP channel pool
│   │   ├── udp_pool.rs               # (~200 lines) UDP channel pool
│   │   ├── guard.rs                  # (~80 lines)  PooledChannelGuard RAII
│   │   └── manager.rs                # (~150 lines) Pool health manager
│   │
│   ├── error.rs                      # (~100 lines) Custom error types
│   └── helper.rs                     # (~80 lines)  Utility functions
│
├── tests/
│   ├── common/
│   │   └── mod.rs                    # (~100 lines) Test utilities, mocks
│   ├── unit/
│   │   ├── socks_parser_test.rs      # (~200 lines) SOCKS5 parsing tests
│   │   ├── socks_auth_test.rs        # (~150 lines) Auth tests
│   │   ├── udp_packet_test.rs        # (~150 lines) UDP packet tests
│   │   ├── ssh_auth_test.rs          # (~180 lines) SSH authentication tests
│   │   ├── ssh_keys_test.rs          # (~150 lines) SSH key handling tests
│   │   └── pool_test.rs              # (~200 lines) Pool logic tests
│   ├── integration/
│   │   ├── control_channel_test.rs   # (~250 lines) Control channel tests
│   │   ├── data_channel_test.rs      # (~200 lines) Data channel tests
│   │   ├── tcp_proxy_test.rs         # (~200 lines) TCP proxy tests
│   │   ├── udp_proxy_test.rs         # (~250 lines) UDP proxy tests
│   │   └── ssh_session_test.rs       # (~250 lines) SSH session tests
│   └── e2e/
│       ├── full_flow_test.rs         # (~300 lines) End-to-end tests
│       └── ssh_flow_test.rs          # (~250 lines) SSH end-to-end tests
│
├── examples/
│   ├── config.toml                   # Example configuration
│   └── simple_client.rs              # (~100 lines) Minimal client example
│
└── benches/
    └── throughput_bench.rs           # (~150 lines) Performance benchmarks
```

### Line Count Enforcement Script

```bash
#!/bin/bash
# scripts/check_line_count.sh

MAX_LINES=600
EXIT_CODE=0

echo "Checking Rust files for line count limit (max: $MAX_LINES)..."

for file in $(find src tests -name "*.rs"); do
    lines=$(wc -l < "$file")
    if [ "$lines" -gt "$MAX_LINES" ]; then
        echo "❌ FAIL: $file has $lines lines (max: $MAX_LINES)"
        EXIT_CODE=1
    else
        echo "✓ OK: $file ($lines lines)"
    fi
done

if [ $EXIT_CODE -eq 0 ]; then
    echo ""
    echo "✅ All files are within the line limit!"
else
    echo ""
    echo "❌ Some files exceed the line limit. Please split them."
fi

exit $EXIT_CODE
```

### Docker Build Configuration

> ⚠️ **MANDATORY**: All builds MUST use the `rust:slim-trixie` Docker image.

#### Dockerfile

```dockerfile
# Dockerfile
# Build stage using rust:slim-trixie
FROM rust:slim-trixie AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source for dependency caching
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source
COPY src ./src
COPY tests ./tests

# Build the application
RUN touch src/main.rs && \
    cargo build --release

# Runtime stage - minimal image
FROM debian:trixie-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/socksrat /usr/local/bin/socksrat

# Create non-root user
RUN useradd -r -s /bin/false socksrat
USER socksrat

ENTRYPOINT ["socksrat"]
CMD ["--help"]
```

#### Docker Compose for Development

```yaml
# docker-compose.yml
version: '3.8'

services:
  # Development container with hot-reload
  dev:
    build:
      context: .
      dockerfile: Dockerfile.dev
    volumes:
      - .:/app
      - cargo-cache:/usr/local/cargo/registry
      - target-cache:/app/target
    working_dir: /app
    command: cargo watch -x "test" -x "run -- -c examples/config.toml"

  # Test runner
  test:
    build:
      context: .
      dockerfile: Dockerfile.dev
    volumes:
      - .:/app
      - cargo-cache:/usr/local/cargo/registry
      - target-cache:/app/target
    working_dir: /app
    command: cargo test --all-features

  # Coverage runner
  coverage:
    build:
      context: .
      dockerfile: Dockerfile.dev
    volumes:
      - .:/app
      - cargo-cache:/usr/local/cargo/registry
      - target-cache:/app/target
    working_dir: /app
    command: cargo tarpaulin --out Html --fail-under 80

volumes:
  cargo-cache:
  target-cache:
```

#### Development Dockerfile

```dockerfile
# Dockerfile.dev
FROM rust:slim-trixie

# Install development dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install development tools
RUN cargo install cargo-watch cargo-tarpaulin

WORKDIR /app

# Keep container running for development
CMD ["bash"]
```

### CI Configuration

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

env:
  DOCKER_IMAGE: rust:slim-trixie

jobs:
  lint-and-check:
    runs-on: ubuntu-latest
    container:
      image: rust:slim-trixie
    steps:
      - uses: actions/checkout@v4

      - name: Install dependencies
        run: |
          apt-get update && apt-get install -y pkg-config libssl-dev

      - name: Check line count
        run: |
          chmod +x scripts/check_line_count.sh
          ./scripts/check_line_count.sh

      - name: Check formatting
        run: cargo fmt -- --check

      - name: Clippy
        run: cargo clippy --all-features -- -D warnings

  test:
    runs-on: ubuntu-latest
    container:
      image: rust:slim-trixie
    steps:
      - uses: actions/checkout@v4

      - name: Install dependencies
        run: |
          apt-get update && apt-get install -y pkg-config libssl-dev

      - name: Run tests
        run: cargo test --all-features --verbose

  coverage:
    runs-on: ubuntu-latest
    container:
      image: rust:slim-trixie
    steps:
      - uses: actions/checkout@v4

      - name: Install dependencies
        run: |
          apt-get update && apt-get install -y pkg-config libssl-dev

      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin

      - name: Run coverage
        run: cargo tarpaulin --out Xml --fail-under 80

      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          files: cobertura.xml
          fail_ci_if_error: true

  build:
    runs-on: ubuntu-latest
    needs: [lint-and-check, test, coverage]
    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build Docker image
        uses: docker/build-push-action@v5
        with:
          context: .
          push: false
          tags: socksrat:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max

  release:
    runs-on: ubuntu-latest
    needs: build
    if: startsWith(github.ref, 'refs/tags/')
    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Login to Docker Hub
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: .
          push: true
          tags: |
            ${{ secrets.DOCKERHUB_USERNAME }}/socksrat:latest
            ${{ secrets.DOCKERHUB_USERNAME }}/socksrat:${{ github.ref_name }}
```

### Makefile for Common Tasks

```makefile
# Makefile
.PHONY: all build test coverage lint clean docker-build docker-run

DOCKER_IMAGE := rust:slim-trixie
APP_NAME := socksrat

all: lint test build

# Build using Docker
build:
	docker run --rm -v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apt-get update && apt-get install -y pkg-config libssl-dev && cargo build --release"

# Run tests using Docker
test:
	docker run --rm -v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apt-get update && apt-get install -y pkg-config libssl-dev && cargo test --all-features"

# Run coverage using Docker
coverage:
	docker run --rm -v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apt-get update && apt-get install -y pkg-config libssl-dev && \
		       cargo install cargo-tarpaulin && \
		       cargo tarpaulin --out Html --fail-under 80"

# Lint using Docker
lint:
	docker run --rm -v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "cargo fmt -- --check && cargo clippy --all-features -- -D warnings"

# Check line count
check-lines:
	./scripts/check_line_count.sh

# Build Docker image
docker-build:
	docker build -t $(APP_NAME):latest .

# Run Docker container
docker-run:
	docker run --rm -it $(APP_NAME):latest

# Clean build artifacts
clean:
	docker run --rm -v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) cargo clean
	rm -rf target/
```

---

## Cargo.toml

```toml
[package]
name = "socksrat"
version = "0.1.0"
edition = "2026"
authors = ["Anthony Rusdi"]
description = "Reverse SOCKS5/SSH tunneling client using rathole protocol"
license = "MIT"

[features]
default = ["native-tls", "noise", "ssh"]

# TLS support
native-tls = ["tokio-native-tls"]
rustls = ["tokio-rustls", "rustls-pemfile", "rustls-native-certs"]

# Noise protocol support
noise = ["snowstorm", "base64"]

# SSH server support
ssh = ["russh", "russh-keys"]

# WebSocket support
websocket-native-tls = ["tokio-tungstenite", "tokio-util", "futures-core", "futures-sink", "native-tls"]
websocket-rustls = ["tokio-tungstenite", "tokio-util", "futures-core", "futures-sink", "rustls"]

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
toml = "0.5"
clap = { version = "4.0", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
log = "0.4"

# Protocol serialization
bincode = "1"
sha2 = "0.10"
lazy_static = "1.4"

# Networking
socket2 = { version = "0.5", features = ["all"] }
backoff = { version = "0.4", features = ["tokio"] }

# SOCKS5 protocol (from fast-socks5, we'll adapt)
tokio-stream = "0.1"

# Optional TLS
tokio-native-tls = { version = "0.3", optional = true }
tokio-rustls = { version = "0.25", optional = true }
rustls-native-certs = { version = "0.7", optional = true }
rustls-pemfile = { version = "2.0", optional = true }

# Optional Noise
snowstorm = { version = "0.4", optional = true, features = ["stream"], default-features = false }
base64 = { version = "0.21", optional = true }

# Optional WebSocket
tokio-tungstenite = { version = "0.20", optional = true }
tokio-util = { version = "0.7", optional = true, features = ["io"] }
futures-core = { version = "0.3", optional = true }
futures-sink = { version = "0.3", optional = true }

# Optional SSH server (russh)
russh = { version = "0.47", optional = true, default-features = false }
russh-keys = { version = "0.47", optional = true }

# Proxy support for outbound connections
async-http-proxy = { version = "1.2", features = ["runtime-tokio", "basic-auth"] }
async-socks5 = "0.5"
url = { version = "2.2", features = ["serde"] }

[dev-dependencies]
tokio-test = "0.4"
env_logger = "0.10"
```

---

## Core Components

### 1. Configuration (`src/config.rs`)

Simplified configuration for client-only mode:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub client: ClientConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
    /// Remote rathole server address (e.g., "server.example.com:2333")
    pub remote_addr: String,

    /// Authentication token for rathole server
    pub token: String,

    /// Transport configuration
    #[serde(default)]
    pub transport: TransportConfig,

    /// Heartbeat timeout in seconds
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout: u64,

    /// Services configuration (SOCKS5, SSH, etc.)
    pub services: ServicesConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServicesConfig {
    /// SOCKS5 service configuration (optional)
    pub socks: Option<ServiceEntry<SocksConfig>>,

    /// SSH service configuration (optional)
    pub ssh: Option<ServiceEntry<SshConfig>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceEntry<T> {
    /// Service name (must match rathole server config)
    pub service_name: String,

    /// Service-specific configuration
    #[serde(flatten)]
    pub config: T,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SocksConfig {
    /// Enable/disable authentication
    #[serde(default)]
    pub auth_required: bool,

    /// Username for SOCKS5 auth
    pub username: Option<String>,

    /// Password for SOCKS5 auth
    pub password: Option<String>,

    /// Allow UDP associate command
    #[serde(default)]
    pub allow_udp: bool,

    /// DNS resolution mode
    #[serde(default = "default_dns_resolve")]
    pub dns_resolve: bool,

    /// Request timeout in seconds
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
}

/// SSH server configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshConfig {
    /// Path to host private key (e.g., "/path/to/host_key")
    pub host_key_path: String,

    /// Allow password authentication
    #[serde(default)]
    pub allow_password_auth: bool,

    /// Allow public key authentication
    #[serde(default = "default_true")]
    pub allow_publickey_auth: bool,

    /// Path to authorized_keys file (for public key auth)
    pub authorized_keys_path: Option<String>,

    /// Users allowed to connect (username -> password mapping for password auth)
    #[serde(default)]
    pub users: HashMap<String, SshUserConfig>,

    /// Connection timeout in seconds
    #[serde(default = "default_ssh_timeout")]
    pub connection_timeout: u64,

    /// Maximum authentication attempts
    #[serde(default = "default_max_auth_attempts")]
    pub max_auth_attempts: u32,

    /// Enable SFTP subsystem
    #[serde(default = "default_true")]
    pub enable_sftp: bool,

    /// Enable shell access
    #[serde(default = "default_true")]
    pub enable_shell: bool,

    /// Enable exec command
    #[serde(default = "default_true")]
    pub enable_exec: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshUserConfig {
    /// Password for this user (if password auth enabled)
    pub password: Option<String>,

    /// Additional authorized keys for this user
    pub authorized_keys: Option<Vec<String>>,

    /// Home directory for this user
    pub home_dir: Option<String>,

    /// Shell for this user
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_true() -> bool { true }
fn default_ssh_timeout() -> u64 { 60 }
fn default_max_auth_attempts() -> u32 { 3 }
fn default_shell() -> String { "/bin/bash".to_string() }

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TransportConfig {
    #[serde(rename = "type", default)]
    pub transport_type: TransportType,

    #[serde(default)]
    pub tcp: TcpConfig,

    pub tls: Option<TlsConfig>,
    pub noise: Option<NoiseConfig>,
    pub websocket: Option<WebsocketConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportType {
    #[default]
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    Tls,
    #[serde(rename = "noise")]
    Noise,
    #[serde(rename = "websocket")]
    Websocket,
}

// ... TcpConfig, TlsConfig, NoiseConfig, WebsocketConfig remain similar to rathole
```

### 2. Protocol Layer (`src/protocol.rs`)

This is largely copied from rathole with minimal modifications:

```rust
// Copy the entire protocol.rs from rathole
// Key structures:
// - Hello enum (ControlChannelHello, DataChannelHello)
// - Auth struct
// - Ack enum
// - ControlChannelCmd enum
// - DataChannelCmd enum
// - Protocol reading/writing functions
```

### 3. Transport Layer (`src/transport/`)

Copy the transport modules from rathole, keeping only client-side functionality:

```rust
// src/transport/mod.rs
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
use std::fmt::Debug;

#[async_trait]
pub trait Transport: Debug + Send + Sync {
    type Stream: 'static + AsyncRead + AsyncWrite + Unpin + Send + Sync + Debug;

    fn new(config: &TransportConfig) -> anyhow::Result<Self> where Self: Sized;
    fn hint(conn: &Self::Stream, opts: SocketOpts);
    async fn connect(&self, addr: &AddrMaybeCached) -> anyhow::Result<Self::Stream>;
    // Note: No bind/accept methods - client only!
}
```

### 4. Main Client Logic (`src/client.rs`)

The heart of the application - adapted from rathole's client:

```rust
use crate::config::ClientConfig;
use crate::protocol::*;
use crate::socks::SocksHandler;
use crate::transport::{Transport, AddrMaybeCached, SocketOpts};
use anyhow::{anyhow, bail, Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, error, info, warn};

pub struct Client<T: Transport> {
    config: ClientConfig,
    transport: Arc<T>,
}

impl<T: 'static + Transport> Client<T> {
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let transport = Arc::new(T::new(&config.transport)?);
        Ok(Client { config, transport })
    }

    pub async fn run(&self, mut shutdown_rx: broadcast::Receiver<bool>) -> Result<()> {
        let control_channel = ControlChannel::new(
            self.config.clone(),
            self.transport.clone(),
        );

        tokio::select! {
            result = control_channel.run() => {
                if let Err(e) = result {
                    error!("Control channel error: {:#}", e);
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received");
            }
        }

        Ok(())
    }
}

struct ControlChannel<T: Transport> {
    config: ClientConfig,
    transport: Arc<T>,
}

impl<T: 'static + Transport> ControlChannel<T> {
    fn new(config: ClientConfig, transport: Arc<T>) -> Self {
        ControlChannel { config, transport }
    }

    async fn run(&self) -> Result<()> {
        loop {
            match self.run_once().await {
                Ok(_) => break,
                Err(e) => {
                    warn!("Control channel error: {:#}. Reconnecting...", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        Ok(())
    }

    async fn run_once(&self) -> Result<()> {
        let mut remote_addr = AddrMaybeCached::new(&self.config.remote_addr);
        remote_addr.resolve().await?;

        let mut conn = self.transport.connect(&remote_addr).await?;
        T::hint(&conn, SocketOpts::for_control_channel());

        // Perform handshake (same as rathole)
        let session_key = self.do_handshake(&mut conn).await?;

        info!("Control channel established");

        // Listen for commands
        loop {
            tokio::select! {
                cmd = read_control_cmd(&mut conn) => {
                    match cmd? {
                        ControlChannelCmd::CreateDataChannel => {
                            let args = DataChannelArgs {
                                session_key: session_key.clone(),
                                remote_addr: remote_addr.clone(),
                                connector: self.transport.clone(),
                                socks_config: self.config.socks.clone(),
                            };
                            tokio::spawn(run_data_channel(args));
                        }
                        ControlChannelCmd::HeartBeat => {
                            debug!("Heartbeat received");
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(self.config.heartbeat_timeout)) => {
                    bail!("Heartbeat timeout");
                }
            }
        }
    }

    async fn do_handshake<S: AsyncRead + AsyncWrite + Unpin>(&self, conn: &mut S) -> Result<Digest> {
        // Send control channel hello
        let service_digest = protocol::digest(self.config.service_name.as_bytes());
        let hello = Hello::ControlChannelHello(CURRENT_PROTO_VERSION, service_digest);
        conn.write_all(&bincode::serialize(&hello)?).await?;
        conn.flush().await?;

        // Read server's hello (contains nonce)
        let nonce = match read_hello(conn).await? {
            Hello::ControlChannelHello(_, d) => d,
            _ => bail!("Unexpected hello type"),
        };

        // Send auth
        let mut concat = Vec::from(self.config.token.as_bytes());
        concat.extend_from_slice(&nonce);
        let session_key = protocol::digest(&concat);
        let auth = Auth(session_key);
        conn.write_all(&bincode::serialize(&auth)?).await?;
        conn.flush().await?;

        // Read ack
        match read_ack(conn).await? {
            Ack::Ok => Ok(session_key),
            other => bail!("Authentication failed: {}", other),
        }
    }
}

/// Service type for routing data channels
#[derive(Clone)]
enum ServiceHandler {
    Socks5(Arc<SocksConfig>),
    Ssh(Arc<SshConfig>, Arc<HostKeys>),
}

struct DataChannelArgs<T: Transport> {
    service_name: String,
    session_key: Digest,
    remote_addr: AddrMaybeCached,
    connector: Arc<T>,
    service_handler: ServiceHandler,
}

async fn run_data_channel<T: Transport>(args: DataChannelArgs<T>) -> Result<()> {
    // Connect to server
    let mut conn = args.connector.connect(&args.remote_addr).await?;

    // Send data channel hello
    let hello = Hello::DataChannelHello(CURRENT_PROTO_VERSION, args.session_key);
    conn.write_all(&bincode::serialize(&hello)?).await?;
    conn.flush().await?;

    // Read command from server
    match read_data_cmd(&mut conn).await? {
        DataChannelCmd::StartForwardTcp => {
            // Route to appropriate handler based on service type
            match &args.service_handler {
                ServiceHandler::Socks5(config) => {
                    // Process as SOCKS5 connection
                    debug!("Routing to SOCKS5 handler for service: {}", args.service_name);
                    handle_socks5_on_stream(conn, config).await?;
                }
                ServiceHandler::Ssh(config, host_keys) => {
                    // Process as SSH connection
                    debug!("Routing to SSH handler for service: {}", args.service_name);
                    handle_ssh_on_stream(conn, config.clone(), host_keys.clone()).await?;
                }
            }
        }
        DataChannelCmd::StartForwardUdp => {
            // UDP only supported for SOCKS5
            match &args.service_handler {
                ServiceHandler::Socks5(config) => {
                    if config.allow_udp {
                        debug!("UDP ASSOCIATE for SOCKS5 service: {}", args.service_name);
                        handle_udp_associate_on_stream(conn, config).await?;
                    } else {
                        warn!("UDP not enabled for SOCKS5 service");
                    }
                }
                ServiceHandler::Ssh(_config, _host_keys) => {
                    warn!("UDP not supported for SSH service");
                }
            }
        }
    }

    Ok(())
}
```

### 5. In-Memory SOCKS5 Handler (`src/socks/handler.rs`)

The critical integration point - processing SOCKS5 on the tunnel stream:

```rust
use crate::config::SocksConfig;
use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

// SOCKS5 constants (from fast-socks5)
const SOCKS5_VERSION: u8 = 0x05;
const SOCKS5_AUTH_METHOD_NONE: u8 = 0x00;
const SOCKS5_AUTH_METHOD_PASSWORD: u8 = 0x02;
const SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE: u8 = 0xff;
const SOCKS5_CMD_TCP_CONNECT: u8 = 0x01;
const SOCKS5_CMD_TCP_BIND: u8 = 0x02;
const SOCKS5_CMD_UDP_ASSOCIATE: u8 = 0x03;
const SOCKS5_ADDR_TYPE_IPV4: u8 = 0x01;
const SOCKS5_ADDR_TYPE_DOMAIN: u8 = 0x03;
const SOCKS5_ADDR_TYPE_IPV6: u8 = 0x04;
const SOCKS5_REPLY_SUCCEEDED: u8 = 0x00;
const SOCKS5_REPLY_GENERAL_FAILURE: u8 = 0x01;
const SOCKS5_REPLY_CONNECTION_NOT_ALLOWED: u8 = 0x02;
const SOCKS5_REPLY_NETWORK_UNREACHABLE: u8 = 0x03;
const SOCKS5_REPLY_HOST_UNREACHABLE: u8 = 0x04;
const SOCKS5_REPLY_CONNECTION_REFUSED: u8 = 0x05;
const SOCKS5_REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;

/// Target address enum
#[derive(Debug, Clone)]
pub enum TargetAddr {
    Ip(SocketAddr),
    Domain(String, u16),
}

impl TargetAddr {
    pub async fn resolve(&self) -> Result<SocketAddr> {
        match self {
            TargetAddr::Ip(addr) => Ok(*addr),
            TargetAddr::Domain(domain, port) => {
                let addr = tokio::net::lookup_host(format!("{}:{}", domain, port))
                    .await?
                    .next()
                    .context("Failed to resolve domain")?;
                Ok(addr)
            }
        }
    }
}

/// Handle SOCKS5 protocol on a stream (the tunnel connection)
///
/// This is the key function that replaces local socket binding.
/// The stream comes directly from the rathole tunnel.
pub async fn handle_socks5_on_stream<S>(mut stream: S, config: &SocksConfig) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // Step 1: Authentication negotiation
    let auth_method = negotiate_auth(&mut stream, config).await?;

    // Step 2: If password auth required, perform it
    if auth_method == SOCKS5_AUTH_METHOD_PASSWORD {
        authenticate_password(&mut stream, config).await?;
    }

    // Step 3: Read the SOCKS5 command
    let (cmd, target_addr) = read_command(&mut stream, config.dns_resolve).await?;

    // Step 4: Execute the command
    match cmd {
        SOCKS5_CMD_TCP_CONNECT => {
            handle_tcp_connect(stream, target_addr, config).await?;
        }
        SOCKS5_CMD_UDP_ASSOCIATE if config.allow_udp => {
            // UDP associate is complex for reverse tunneling
            // For now, send not supported
            error!("UDP ASSOCIATE not implemented for reverse tunnel");
            send_reply(&mut stream, SOCKS5_REPLY_COMMAND_NOT_SUPPORTED, None).await?;
        }
        _ => {
            send_reply(&mut stream, SOCKS5_REPLY_COMMAND_NOT_SUPPORTED, None).await?;
        }
    }

    Ok(())
}

async fn negotiate_auth<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    config: &SocksConfig,
) -> Result<u8> {
    // Read version and number of methods
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let num_methods = buf[1];

    if version != SOCKS5_VERSION {
        anyhow::bail!("Unsupported SOCKS version: {}", version);
    }

    // Read available methods
    let mut methods = vec![0u8; num_methods as usize];
    stream.read_exact(&mut methods).await?;

    // Select authentication method
    let selected_method = if config.auth_required {
        if methods.contains(&SOCKS5_AUTH_METHOD_PASSWORD) {
            SOCKS5_AUTH_METHOD_PASSWORD
        } else {
            SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE
        }
    } else {
        if methods.contains(&SOCKS5_AUTH_METHOD_NONE) {
            SOCKS5_AUTH_METHOD_NONE
        } else if methods.contains(&SOCKS5_AUTH_METHOD_PASSWORD) && config.username.is_some() {
            SOCKS5_AUTH_METHOD_PASSWORD
        } else {
            SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE
        }
    };

    // Send selected method
    stream.write_all(&[SOCKS5_VERSION, selected_method]).await?;
    stream.flush().await?;

    if selected_method == SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE {
        anyhow::bail!("No acceptable authentication method");
    }

    Ok(selected_method)
}

async fn authenticate_password<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    config: &SocksConfig,
) -> Result<()> {
    // Read auth version
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;
    let _version = buf[0];
    let username_len = buf[1] as usize;

    // Read username
    let mut username = vec![0u8; username_len];
    stream.read_exact(&mut username).await?;
    let username = String::from_utf8(username)?;

    // Read password length and password
    let mut buf = [0u8; 1];
    stream.read_exact(&mut buf).await?;
    let password_len = buf[0] as usize;

    let mut password = vec![0u8; password_len];
    stream.read_exact(&mut password).await?;
    let password = String::from_utf8(password)?;

    // Verify credentials
    let valid = config.username.as_ref() == Some(&username)
        && config.password.as_ref() == Some(&password);

    if valid {
        stream.write_all(&[1, 0]).await?; // Success
        stream.flush().await?;
        Ok(())
    } else {
        stream.write_all(&[1, 1]).await?; // Failure
        stream.flush().await?;
        anyhow::bail!("Authentication failed")
    }
}

async fn read_command<S: AsyncRead + Unpin>(
    stream: &mut S,
    resolve_dns: bool,
) -> Result<(u8, TargetAddr)> {
    // Read: VER CMD RSV ATYP
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let cmd = buf[1];
    let _rsv = buf[2];
    let atyp = buf[3];

    if version != SOCKS5_VERSION {
        anyhow::bail!("Unsupported SOCKS version in command: {}", version);
    }

    // Read target address based on type
    let target_addr = match atyp {
        SOCKS5_ADDR_TYPE_IPV4 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let mut port = [0u8; 2];
            stream.read_exact(&mut port).await?;
            let port = u16::from_be_bytes(port);
            TargetAddr::Ip(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::from(addr)),
                port,
            ))
        }
        SOCKS5_ADDR_TYPE_DOMAIN => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let len = len[0] as usize;

            let mut domain = vec![0u8; len];
            stream.read_exact(&mut domain).await?;
            let domain = String::from_utf8(domain)?;

            let mut port = [0u8; 2];
            stream.read_exact(&mut port).await?;
            let port = u16::from_be_bytes(port);

            if resolve_dns {
                let addr = TargetAddr::Domain(domain, port);
                TargetAddr::Ip(addr.resolve().await?)
            } else {
                TargetAddr::Domain(domain, port)
            }
        }
        SOCKS5_ADDR_TYPE_IPV6 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            let mut port = [0u8; 2];
            stream.read_exact(&mut port).await?;
            let port = u16::from_be_bytes(port);
            TargetAddr::Ip(SocketAddr::new(
                IpAddr::V6(std::net::Ipv6Addr::from(addr)),
                port,
            ))
        }
        _ => anyhow::bail!("Unsupported address type: {}", atyp),
    };

    Ok((cmd, target_addr))
}

async fn send_reply<S: AsyncWrite + Unpin>(
    stream: &mut S,
    reply_code: u8,
    bind_addr: Option<SocketAddr>,
) -> Result<()> {
    let bind_addr = bind_addr.unwrap_or_else(|| {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
    });

    let mut reply = vec![
        SOCKS5_VERSION,
        reply_code,
        0x00, // Reserved
    ];

    match bind_addr {
        SocketAddr::V4(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV4);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV6);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
    }

    stream.write_all(&reply).await?;
    stream.flush().await?;

    Ok(())
}

async fn handle_tcp_connect<S: AsyncRead + AsyncWrite + Unpin>(
    mut client_stream: S,
    target_addr: TargetAddr,
    config: &SocksConfig,
) -> Result<()> {
    let timeout_duration = Duration::from_secs(config.request_timeout);

    // Resolve address if needed
    let socket_addr = match &target_addr {
        TargetAddr::Ip(addr) => *addr,
        TargetAddr::Domain(domain, port) => {
            tokio::net::lookup_host(format!("{}:{}", domain, port))
                .await?
                .next()
                .context("Failed to resolve domain")?
        }
    };

    debug!("Connecting to target: {}", socket_addr);

    // Connect to target with timeout
    let target_stream = match tokio::time::timeout(
        timeout_duration,
        TcpStream::connect(socket_addr),
    ).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            let reply_code = match e.kind() {
                std::io::ErrorKind::ConnectionRefused => SOCKS5_REPLY_CONNECTION_REFUSED,
                std::io::ErrorKind::TimedOut => SOCKS5_REPLY_HOST_UNREACHABLE,
                _ => SOCKS5_REPLY_GENERAL_FAILURE,
            };
            send_reply(&mut client_stream, reply_code, None).await?;
            return Err(e.into());
        }
        Err(_) => {
            send_reply(&mut client_stream, SOCKS5_REPLY_HOST_UNREACHABLE, None).await?;
            anyhow::bail!("Connection timeout");
        }
    };

    // Get local address for reply
    let local_addr = target_stream.local_addr().ok();

    // Send success reply
    send_reply(&mut client_stream, SOCKS5_REPLY_SUCCEEDED, local_addr).await?;

    info!("SOCKS5 tunnel established to {}", socket_addr);

    // Bidirectional copy
    let (mut client_read, mut client_write) = tokio::io::split(client_stream);
    let (mut target_read, mut target_write) = tokio::io::split(target_stream);

    let client_to_target = tokio::io::copy(&mut client_read, &mut target_write);
    let target_to_client = tokio::io::copy(&mut target_read, &mut client_write);

    tokio::select! {
        result = client_to_target => {
            debug!("Client to target finished: {:?}", result);
        }
        result = target_to_client => {
            debug!("Target to client finished: {:?}", result);
        }
    }

    Ok(())
}
```

### 6. In-Memory SSH Handler (`src/ssh/handler.rs`)

The SSH handler uses `russh` to process SSH connections directly from tunnel streams,
without binding to a local port. This mirrors the SOCKS5 handler architecture.

```rust
//! SSH Handler - Process SSH sessions from tunnel streams
//!
//! Uses russh's `run_stream()` function to handle SSH protocol
//! on any AsyncRead + AsyncWrite stream (tunnel data channels).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::server::{Auth, Handle, Handler, Msg, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodSet};
use russh_keys::key::PublicKey;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::SshConfig;
use crate::ssh::auth::{AuthContext, AuthResult};
use crate::ssh::keys::HostKeys;
use crate::ssh::session::SshSession;

/// Handle SSH protocol on a tunnel stream
///
/// This is the key integration point - instead of accepting connections
/// on a local TCP socket, we pass the tunnel stream directly to russh.
pub async fn handle_ssh_on_stream<S>(
    stream: S,
    config: Arc<SshConfig>,
    host_keys: Arc<HostKeys>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    debug!("Starting SSH session on tunnel stream");

    // Build russh server config
    let server_config = build_server_config(&config, &host_keys)?;

    // Create our handler
    let handler = SocksRatSshHandler::new(config.clone());

    // Run SSH protocol on the stream
    // This is equivalent to russh::server::run_stream()
    let session = russh::server::run_stream(
        Arc::new(server_config),
        stream,
        handler,
    ).await.context("Failed to run SSH session")?;

    // Wait for session to complete
    session.await.context("SSH session error")?;

    info!("SSH session completed");
    Ok(())
}

/// Build russh server configuration from our SshConfig
fn build_server_config(
    config: &SshConfig,
    host_keys: &HostKeys,
) -> Result<russh::server::Config> {
    let mut methods = MethodSet::empty();

    if config.allow_publickey_auth {
        methods |= MethodSet::PUBLICKEY;
    }
    if config.allow_password_auth {
        methods |= MethodSet::PASSWORD;
    }

    Ok(russh::server::Config {
        methods,
        keys: host_keys.get_keys()?,
        auth_rejection_time: std::time::Duration::from_secs(1),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        connection_timeout: Some(std::time::Duration::from_secs(
            config.connection_timeout
        )),
        ..Default::default()
    })
}

/// SSH Handler implementation for SocksRat
///
/// Implements russh::server::Handler to process SSH sessions
pub struct SocksRatSshHandler {
    config: Arc<SshConfig>,
    auth_context: AuthContext,
    sessions: Arc<Mutex<HashMap<ChannelId, SshSession>>>,
    auth_attempts: u32,
}

impl SocksRatSshHandler {
    pub fn new(config: Arc<SshConfig>) -> Self {
        Self {
            auth_context: AuthContext::new(config.clone()),
            config,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            auth_attempts: 0,
        }
    }
}

/// Handler trait implementation for russh server
#[async_trait]
impl Handler for SocksRatSshHandler {
    type Error = anyhow::Error;

    /// Called when client requests password authentication
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<Auth, Self::Error> {
        if !self.config.allow_password_auth {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
            });
        }

        self.auth_attempts += 1;
        if self.auth_attempts > self.config.max_auth_attempts {
            warn!("Max auth attempts exceeded for user: {}", user);
            return Ok(Auth::Reject {
                proceed_with_methods: None,
            });
        }

        match self.auth_context.verify_password(user, password).await {
            AuthResult::Success => {
                info!("Password auth successful for user: {}", user);
                Ok(Auth::Accept)
            }
            AuthResult::Failure => {
                warn!("Password auth failed for user: {}", user);
                Ok(Auth::Reject {
                    proceed_with_methods: Some(MethodSet::all()),
                })
            }
        }
    }

    /// Called when client requests public key authentication
    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        if !self.config.allow_publickey_auth {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
            });
        }

        match self.auth_context.verify_publickey(user, public_key).await {
            AuthResult::Success => {
                info!("Public key auth successful for user: {}", user);
                Ok(Auth::Accept)
            }
            AuthResult::Failure => {
                warn!("Public key auth failed for user: {}", user);
                Ok(Auth::Reject {
                    proceed_with_methods: Some(MethodSet::all()),
                })
            }
        }
    }

    /// Called when a new session channel is opened
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let channel_id = channel.id();
        debug!("Channel open session: {:?}", channel_id);

        // Create a new session handler
        let ssh_session = SshSession::new(
            channel,
            self.config.clone(),
        );

        self.sessions.lock().await.insert(channel_id, ssh_session);

        Ok(true)
    }

    /// Called when client requests a PTY
    async fn pty_request(
        &mut self,
        channel_id: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(
            "PTY request: term={}, cols={}, rows={}",
            term, col_width, row_height
        );

        if let Some(ssh_session) = self.sessions.lock().await.get_mut(&channel_id) {
            ssh_session.set_pty(term, col_width, row_height, pix_width, pix_height);
        }

        Ok(())
    }

    /// Called when client requests shell access
    async fn shell_request(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.config.enable_shell {
            warn!("Shell access disabled");
            return Ok(());
        }

        debug!("Shell request for channel: {:?}", channel_id);

        if let Some(ssh_session) = self.sessions.lock().await.get_mut(&channel_id) {
            ssh_session.start_shell(session).await?;
        }

        Ok(())
    }

    /// Called when client requests command execution
    async fn exec_request(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.config.enable_exec {
            warn!("Exec access disabled");
            return Ok(());
        }

        let command = String::from_utf8_lossy(data);
        debug!("Exec request: {}", command);

        if let Some(ssh_session) = self.sessions.lock().await.get_mut(&channel_id) {
            ssh_session.exec_command(&command, session).await?;
        }

        Ok(())
    }

    /// Called when client requests SFTP subsystem
    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Subsystem request: {}", name);

        if name == "sftp" && self.config.enable_sftp {
            if let Some(ssh_session) = self.sessions.lock().await.get_mut(&channel_id) {
                ssh_session.start_sftp(session).await?;
            }
        }

        Ok(())
    }

    /// Called when data is received on a channel
    async fn data(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(ssh_session) = self.sessions.lock().await.get_mut(&channel_id) {
            ssh_session.handle_data(data, session).await?;
        }

        Ok(())
    }

    /// Called when channel EOF is received
    async fn channel_eof(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Channel EOF: {:?}", channel_id);

        if let Some(mut ssh_session) = self.sessions.lock().await.remove(&channel_id) {
            ssh_session.close().await?;
        }

        Ok(())
    }

    /// Called when channel is closed
    async fn channel_close(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Channel close: {:?}", channel_id);

        self.sessions.lock().await.remove(&channel_id);

        Ok(())
    }
}
```

### 7. Main Entry Point (`src/main.rs`)

```rust
use anyhow::Result;
use clap::Parser;
use socksrat::config::Config;
use socksrat::client::run_client;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(name = "socksrat")]
#[command(author, version, about = "Reverse SOCKS5 tunneling client", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    let level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load configuration
    let config_str = std::fs::read_to_string(&args.config)?;
    let config: Config = toml::from_str(&config_str)?;

    info!("Starting SocksRat client");
    info!("Connecting to: {}", config.client.remote_addr);

    // Setup shutdown signal
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        info!("Shutdown signal received");
        let _ = shutdown_tx.send(true);
    });

    // Run client
    run_client(config, shutdown_rx).await
}
```

---

## Example Configuration (`examples/config.toml`)

```toml
[client]
# Remote rathole server address
remote_addr = "server.example.com:2333"

# Authentication token (must match server)
token = "your-secret-token"

# Heartbeat timeout in seconds
heartbeat_timeout = 40

[client.transport]
type = "tcp"  # or "tls", "noise", "websocket"

[client.transport.tcp]
nodelay = true
keepalive_secs = 20
keepalive_interval = 8

# Uncomment for TLS transport
# [client.transport.tls]
# hostname = "server.example.com"
# trusted_root = "/path/to/ca.crt"

# Uncomment for Noise transport
# [client.transport.noise]
# pattern = "Noise_NK_25519_ChaChaPoly_BLAKE2s"
# remote_public_key = "base64-encoded-key"

# =====================
# SOCKS5 Service Config
# =====================
[client.services.socks]
# Service name (must match server configuration)
service_name = "socks5"

# Require SOCKS5 authentication
auth_required = false

# Credentials (if auth_required = true)
# username = "user"
# password = "pass"

# Allow UDP ASSOCIATE command
allow_udp = true

# Resolve DNS on the client side
dns_resolve = true

# Connection timeout in seconds
request_timeout = 10

# =====================
# SSH Service Config
# =====================
[client.services.ssh]
# Service name (must match server configuration)
service_name = "ssh"

# Path to host private key (will be generated if not exists)
host_key_path = "/etc/socksrat/host_key"

# Authentication methods
allow_password_auth = true
allow_publickey_auth = true

# Path to authorized_keys file (OpenSSH format)
authorized_keys_path = "/etc/socksrat/authorized_keys"

# Connection timeout in seconds
connection_timeout = 60

# Maximum authentication attempts
max_auth_attempts = 3

# Enable subsystems and features
enable_sftp = true
enable_shell = true
enable_exec = true

# User configuration
[client.services.ssh.users.admin]
password = "admin-password"  # Only used if allow_password_auth = true
home_dir = "/home/admin"
shell = "/bin/bash"

[client.services.ssh.users.guest]
password = "guest-password"
home_dir = "/tmp/guest"
shell = "/bin/sh"
```

---

## Rathole Server Configuration

For the remote rathole server, configure both SOCKS5 and SSH services:

```toml
[server]
bind_addr = "0.0.0.0:2333"
default_token = "your-secret-token"

# SOCKS5 service - clients connect here to use SOCKS5 proxy
[server.services.socks5]
type = "tcp"
bind_addr = "0.0.0.0:1080"

# SSH service - clients connect here to use SSH server
[server.services.ssh]
type = "tcp"
bind_addr = "0.0.0.0:2222"
```

---

## Data Flow

### Connection Establishment

```
1. SocksRat Client                    Rathole Server                 SOCKS5 Client
        |                                   |                              |
        |---(TCP/TLS/Noise/WS connect)----->|                              |
        |                                   |                              |
        |<---(Control Channel Hello)--------|                              |
        |                                   |                              |
        |---(Auth)------------------------->|                              |
        |                                   |                              |
        |<---(Ack: OK)----------------------|                              |
        |                                   |                              |
        |   [Control Channel Established]   |                              |
        |                                   |                              |
```

### SOCKS5 Request Handling

```
2. SocksRat Client                    Rathole Server                 SOCKS5 Client
        |                                   |                              |
        |                                   |<---(SOCKS5 connect)----------|
        |                                   |                              |
        |<---(CreateDataChannel cmd)--------|                              |
        |                                   |                              |
        |---(Data Channel Hello)----------->|                              |
        |                                   |                              |
        |<---(StartForwardTcp)--------------|                              |
        |                                   |                              |
        |<===(SOCKS5 handshake via tunnel)==|<===(SOCKS5 handshake)========|
        |                                   |                              |
        |   [SOCKS5 auth negotiation]       |                              |
        |                                   |                              |
        |<===(SOCKS5 CONNECT request)=======|<===(SOCKS5 CONNECT)==========|
        |                                   |                              |
        |---(TCP connect to target)---------|                              |
        |                                   |                              |
        |===(SOCKS5 reply via tunnel)======>|====(SOCKS5 reply)==========>|
        |                                   |                              |
        |<==(bidirectional data relay)=====>|<===(bidirectional relay)====>|
        |                                   |                              |
```

---

## Key Integration Points

### 1. Replacing `run_data_channel_for_tcp` (from rathole)

**Original rathole code:**
```rust
async fn run_data_channel_for_tcp<T: Transport>(
    mut conn: T::Stream,
    local_addr: &str,
) -> Result<()> {
    let mut local = TcpStream::connect(local_addr).await?;
    let _ = copy_bidirectional(&mut conn, &mut local).await;
    Ok(())
}
```

**New SocksRat code:**
```rust
async fn run_data_channel_for_tcp<T: Transport>(
    conn: T::Stream,
    socks_config: &SocksConfig,
) -> Result<()> {
    // Instead of connecting to a local address,
    // process the tunnel stream as a SOCKS5 request
    handle_socks5_on_stream(conn, socks_config).await
}
```

### 2. The `handle_socks5_on_stream` Function

This is the critical bridge between rathole's transport and fast-socks5's protocol handling:

- Takes the tunnel stream directly (no local socket binding)
- Implements SOCKS5 protocol in-memory
- Performs authentication if configured
- Reads CONNECT command and target address
- Establishes outbound connection to actual target
- Relays data bidirectionally

### 3. Authentication Flow

```
Tunnel Stream                 SocksRat Handler              Target
    |                              |                          |
    |--[SOCKS5 Ver + Methods]----->|                          |
    |                              |                          |
    |<-[Selected Method]-----------|                          |
    |                              |                          |
    |--[Username/Password]-------->| (if auth required)       |
    |                              |                          |
    |<-[Auth Result]---------------|                          |
    |                              |                          |
    |--[CONNECT cmd + target]----->|                          |
    |                              |---[TCP Connect]--------->|
    |                              |<--[Connected]------------|
    |<-[SOCKS5 Reply]--------------|                          |
    |                              |                          |
    |<======[Bidirectional Relay]======>                      |
```

### 4. SSH Server Integration (using russh)

Similar to SOCKS5, the SSH server processes tunnel streams directly using `russh::server::run_stream()`:

**Original rathole code (connects to local SSH server):**
```rust
async fn run_data_channel_for_tcp<T: Transport>(
    mut conn: T::Stream,
    local_addr: &str,  // e.g., "127.0.0.1:22"
) -> Result<()> {
    let mut local = TcpStream::connect(local_addr).await?;
    let _ = copy_bidirectional(&mut conn, &mut local).await;
    Ok(())
}
```

**New SocksRat code (embedded SSH server):**
```rust
async fn run_data_channel_for_ssh<T: Transport>(
    conn: T::Stream,
    ssh_config: &SshConfig,
    host_keys: &HostKeys,
) -> Result<()> {
    // Instead of connecting to a local SSH server,
    // process the tunnel stream as an SSH connection
    handle_ssh_on_stream(conn, ssh_config, host_keys).await
}
```

### 5. The `handle_ssh_on_stream` Function

This is the critical bridge between rathole's transport and russh's SSH protocol:

- Takes the tunnel stream directly (no local socket binding)
- Passes stream to `russh::server::run_stream()` for SSH protocol handling
- Implements authentication (password and/or public key)
- Handles channel requests (shell, exec, SFTP)
- Processes data on channels bidirectionally

### 6. SSH Authentication Flow

```
Tunnel Stream                 SocksRat SSH Handler          Local Shell/Exec
    |                              |                              |
    |--[SSH Protocol Version]----->|                              |
    |<-[SSH Protocol Version]------|                              |
    |                              |                              |
    |--[Key Exchange Init]-------->|                              |
    |<-[Key Exchange Reply]--------|                              |
    |                              |                              |
    |--[New Keys]----------------->|                              |
    |<-[New Keys]------------------|                              |
    |                              |                              |
    |--[Service Request: auth]---->|                              |
    |<-[Service Accept]------------|                              |
    |                              |                              |
    |--[Auth: publickey/password]->| (verify against config)      |
    |<-[Auth Success]--------------|                              |
    |                              |                              |
    |--[Channel Open: session]---->|                              |
    |<-[Channel Open Confirm]------|                              |
    |                              |                              |
    |--[PTY Request]-------------->| (optional)                   |
    |<-[Success]-------------------|                              |
    |                              |                              |
    |--[Shell/Exec Request]------->|---[spawn process]----------->|
    |<-[Success]-------------------|                              |
    |                              |                              |
    |<======[Bidirectional Data]=====>                            |
```

---

## UDP ASSOCIATE Implementation

UDP ASSOCIATE is a mandatory feature that enables DNS queries, gaming protocols, and other UDP-based traffic through the SOCKS5 tunnel.

### UDP Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              UDP ASSOCIATE FLOW                                 │
│                                                                                 │
│  SOCKS5 Client              Rathole Server              SocksRat Client         │
│       │                          │                           │                  │
│       │──[UDP ASSOCIATE cmd]────►│───[via tunnel]───────────►│                  │
│       │                          │                           │                  │
│       │◄─[BND.ADDR:PORT reply]───│◄──[via tunnel]────────────│                  │
│       │                          │                           │                  │
│       │     ┌────────────────────┼───────────────────────────┼──────────┐       │
│       │     │         UDP Data Channel (separate tunnel)     │          │       │
│       │     │                    │                           │          │       │
│       │═════│═[UDP datagram]════►│═══[encapsulated]═════════►│          │       │
│       │     │                    │                           │──►Target │       │
│       │◄════│═[UDP response]═════│◄══[encapsulated]══════════│◄──       │       │
│       │     │                    │                           │          │       │
│       │     └────────────────────┼───────────────────────────┼──────────┘       │
│       │                          │                           │                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### UDP ASSOCIATE Handler (`src/socks/udp_relay.rs`)

```rust
use crate::config::SocksConfig;
use crate::pool::ChannelPool;
use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// UDP packet header as per RFC 1928
/// +----+------+------+----------+----------+----------+
/// |RSV | FRAG | ATYP | DST.ADDR | DST.PORT |   DATA   |
/// +----+------+------+----------+----------+----------+
/// | 2  |  1   |  1   | Variable |    2     | Variable |
/// +----+------+------+----------+----------+----------+

const UDP_BUFFER_SIZE: usize = 65535;
const UDP_TIMEOUT_SECS: u64 = 120;
const UDP_SENDQ_SIZE: usize = 256;

/// Parsed UDP SOCKS5 header
#[derive(Debug, Clone)]
pub struct UdpHeader {
    pub frag: u8,
    pub target_addr: TargetAddr,
}

/// UDP port mapping for tracking client sessions
type UdpPortMap = Arc<RwLock<HashMap<SocketAddr, mpsc::Sender<Bytes>>>>;

/// Handle UDP ASSOCIATE command on the tunnel stream
///
/// This creates a virtual UDP relay that:
/// 1. Uses a separate data channel for UDP traffic
/// 2. Encapsulates UDP datagrams through the tunnel
/// 3. Maintains session state for bidirectional communication
pub async fn handle_udp_associate<S, T>(
    mut control_stream: S,
    config: &SocksConfig,
    channel_pool: Arc<ChannelPool<T>>,
    client_indicated_addr: TargetAddr,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    T: crate::transport::Transport,
{
    // Acquire a dedicated data channel for UDP traffic from the pool
    let udp_channel = channel_pool
        .acquire_udp_channel()
        .await
        .context("Failed to acquire UDP data channel")?;

    // Create the UDP relay state
    let port_map: UdpPortMap = Arc::new(RwLock::new(HashMap::new()));

    // Channel for outbound UDP traffic (client -> target)
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<UdpTraffic>(UDP_SENDQ_SIZE);

    // Send success reply with a virtual bind address
    // In reverse tunnel mode, we use a placeholder since there's no real local binding
    let virtual_bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
    send_udp_reply(&mut control_stream, SOCKS5_REPLY_SUCCEEDED, virtual_bind_addr).await?;

    info!("UDP ASSOCIATE session started");

    // Split the UDP data channel
    let (mut udp_read, mut udp_write) = tokio::io::split(udp_channel);

    // Task: Forward outbound UDP traffic through the tunnel
    let outbound_task = tokio::spawn(async move {
        while let Some(traffic) = outbound_rx.recv().await {
            if let Err(e) = write_udp_traffic(&mut udp_write, &traffic).await {
                debug!("Outbound UDP write error: {:?}", e);
                break;
            }
        }
    });

    // Task: Process inbound UDP traffic from the tunnel
    let port_map_clone = port_map.clone();
    let outbound_tx_clone = outbound_tx.clone();
    let inbound_task = tokio::spawn(async move {
        let mut buf = BytesMut::with_capacity(UDP_BUFFER_SIZE);

        loop {
            match read_udp_traffic(&mut udp_read, &mut buf).await {
                Ok(traffic) => {
                    if let Err(e) = handle_inbound_udp(
                        traffic,
                        &port_map_clone,
                        outbound_tx_clone.clone(),
                    ).await {
                        debug!("Inbound UDP handling error: {:?}", e);
                    }
                }
                Err(e) => {
                    debug!("Inbound UDP read error: {:?}", e);
                    break;
                }
            }
        }
    });

    // Task: Monitor the control stream for termination
    // When the TCP control connection closes, the UDP association terminates
    let control_task = tokio::spawn(async move {
        let mut buf = [0u8; 1];
        loop {
            match control_stream.read(&mut buf).await {
                Ok(0) => {
                    debug!("Control stream closed, terminating UDP association");
                    break;
                }
                Ok(_) => {
                    warn!("Unexpected data on UDP control stream");
                }
                Err(e) => {
                    debug!("Control stream error: {:?}", e);
                    break;
                }
            }
        }
    });

    // Wait for any task to complete (which terminates the session)
    tokio::select! {
        _ = outbound_task => debug!("Outbound task completed"),
        _ = inbound_task => debug!("Inbound task completed"),
        _ = control_task => debug!("Control task completed"),
    }

    info!("UDP ASSOCIATE session ended");
    Ok(())
}

/// UDP traffic structure for tunnel encapsulation
#[derive(Debug)]
pub struct UdpTraffic {
    pub from: SocketAddr,
    pub target: TargetAddr,
    pub data: Bytes,
}

/// Write UDP traffic to the tunnel (with framing)
async fn write_udp_traffic<W: AsyncWrite + Unpin>(
    writer: &mut W,
    traffic: &UdpTraffic,
) -> Result<()> {
    // Frame format:
    // [2 bytes: total length][header][data]
    let header = encode_udp_header(&traffic.target)?;
    let total_len = (header.len() + traffic.data.len()) as u16;

    writer.write_all(&total_len.to_be_bytes()).await?;
    writer.write_all(&header).await?;
    writer.write_all(&traffic.data).await?;
    writer.flush().await?;

    Ok(())
}

/// Read UDP traffic from the tunnel (with framing)
async fn read_udp_traffic<R: AsyncRead + Unpin>(
    reader: &mut R,
    buf: &mut BytesMut,
) -> Result<UdpTraffic> {
    // Read frame length
    let mut len_buf = [0u8; 2];
    reader.read_exact(&mut len_buf).await?;
    let total_len = u16::from_be_bytes(len_buf) as usize;

    // Read frame data
    buf.resize(total_len, 0);
    reader.read_exact(buf).await?;

    // Parse header and extract data
    let (header, data) = parse_udp_frame(buf)?;

    Ok(UdpTraffic {
        from: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0), // Set by context
        target: header.target_addr,
        data: Bytes::copy_from_slice(data),
    })
}

/// Handle inbound UDP packet from tunnel
async fn handle_inbound_udp(
    traffic: UdpTraffic,
    port_map: &UdpPortMap,
    outbound_tx: mpsc::Sender<UdpTraffic>,
) -> Result<()> {
    // Resolve target address
    let target_addr = traffic.target.resolve().await?;

    // Check if we have an existing forwarder for this target
    let map = port_map.read().await;

    if let Some(tx) = map.get(&target_addr) {
        // Forward to existing session
        let _ = tx.send(traffic.data).await;
    } else {
        // Drop read lock and create new forwarder
        drop(map);

        // Create UDP socket for this target
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(target_addr).await?;

        // Create channel for this session
        let (session_tx, session_rx) = mpsc::channel(UDP_SENDQ_SIZE);

        // Register in port map
        let mut map = port_map.write().await;
        map.insert(target_addr, session_tx.clone());
        drop(map);

        // Spawn forwarder task
        let port_map_clone = port_map.clone();
        tokio::spawn(run_udp_forwarder(
            socket,
            session_rx,
            outbound_tx,
            target_addr,
            port_map_clone,
        ));

        // Send initial packet
        let _ = session_tx.send(traffic.data).await;
    }

    Ok(())
}

/// Run a UDP forwarder for a specific target
async fn run_udp_forwarder(
    socket: UdpSocket,
    mut inbound_rx: mpsc::Receiver<Bytes>,
    outbound_tx: mpsc::Sender<UdpTraffic>,
    target: SocketAddr,
    port_map: UdpPortMap,
) -> Result<()> {
    let mut buf = vec![0u8; UDP_BUFFER_SIZE];

    loop {
        tokio::select! {
            // Receive data to send to target
            Some(data) = inbound_rx.recv() => {
                if let Err(e) = socket.send(&data).await {
                    debug!("UDP send error: {:?}", e);
                    break;
                }
            }

            // Receive response from target
            result = socket.recv(&mut buf) => {
                match result {
                    Ok(len) => {
                        let traffic = UdpTraffic {
                            from: target,
                            target: TargetAddr::Ip(target),
                            data: Bytes::copy_from_slice(&buf[..len]),
                        };

                        if outbound_tx.send(traffic).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("UDP recv error: {:?}", e);
                        break;
                    }
                }
            }

            // Timeout - clean up idle sessions
            _ = tokio::time::sleep(Duration::from_secs(UDP_TIMEOUT_SECS)) => {
                debug!("UDP session timeout for {}", target);
                break;
            }
        }
    }

    // Clean up port map entry
    let mut map = port_map.write().await;
    map.remove(&target);

    debug!("UDP forwarder for {} terminated", target);
    Ok(())
}

/// Encode target address into SOCKS5 UDP header format
fn encode_udp_header(target: &TargetAddr) -> Result<Vec<u8>> {
    let mut header = vec![0u8, 0u8, 0u8]; // RSV (2 bytes) + FRAG (1 byte)

    match target {
        TargetAddr::Ip(SocketAddr::V4(addr)) => {
            header.push(SOCKS5_ADDR_TYPE_IPV4);
            header.extend_from_slice(&addr.ip().octets());
            header.extend_from_slice(&addr.port().to_be_bytes());
        }
        TargetAddr::Ip(SocketAddr::V6(addr)) => {
            header.push(SOCKS5_ADDR_TYPE_IPV6);
            header.extend_from_slice(&addr.ip().octets());
            header.extend_from_slice(&addr.port().to_be_bytes());
        }
        TargetAddr::Domain(domain, port) => {
            header.push(SOCKS5_ADDR_TYPE_DOMAIN);
            header.push(domain.len() as u8);
            header.extend_from_slice(domain.as_bytes());
            header.extend_from_slice(&port.to_be_bytes());
        }
    }

    Ok(header)
}

/// Parse SOCKS5 UDP frame and extract header + data
fn parse_udp_frame(buf: &[u8]) -> Result<(UdpHeader, &[u8])> {
    if buf.len() < 4 {
        anyhow::bail!("UDP frame too short");
    }

    let frag = buf[2];
    let atyp = buf[3];

    let (target_addr, header_len) = match atyp {
        SOCKS5_ADDR_TYPE_IPV4 => {
            if buf.len() < 10 {
                anyhow::bail!("IPv4 UDP frame too short");
            }
            let ip = Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            (TargetAddr::Ip(SocketAddr::new(IpAddr::V4(ip), port)), 10)
        }
        SOCKS5_ADDR_TYPE_IPV6 => {
            if buf.len() < 22 {
                anyhow::bail!("IPv6 UDP frame too short");
            }
            let mut ip_bytes = [0u8; 16];
            ip_bytes.copy_from_slice(&buf[4..20]);
            let ip = std::net::Ipv6Addr::from(ip_bytes);
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            (TargetAddr::Ip(SocketAddr::new(IpAddr::V6(ip), port)), 22)
        }
        SOCKS5_ADDR_TYPE_DOMAIN => {
            let domain_len = buf[4] as usize;
            if buf.len() < 5 + domain_len + 2 {
                anyhow::bail!("Domain UDP frame too short");
            }
            let domain = String::from_utf8(buf[5..5 + domain_len].to_vec())?;
            let port = u16::from_be_bytes([buf[5 + domain_len], buf[6 + domain_len]]);
            (TargetAddr::Domain(domain, port), 7 + domain_len)
        }
        _ => anyhow::bail!("Unknown address type: {}", atyp),
    };

    Ok((UdpHeader { frag, target_addr }, &buf[header_len..]))
}

/// Send UDP ASSOCIATE reply
async fn send_udp_reply<S: AsyncWrite + Unpin>(
    stream: &mut S,
    reply_code: u8,
    bind_addr: SocketAddr,
) -> Result<()> {
    let mut reply = vec![
        SOCKS5_VERSION,
        reply_code,
        0x00, // Reserved
    ];

    match bind_addr {
        SocketAddr::V4(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV4);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            reply.push(SOCKS5_ADDR_TYPE_IPV6);
            reply.extend_from_slice(&addr.ip().octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
        }
    }

    stream.write_all(&reply).await?;
    stream.flush().await?;

    Ok(())
}

// SOCKS5 constants
const SOCKS5_VERSION: u8 = 0x05;
const SOCKS5_ADDR_TYPE_IPV4: u8 = 0x01;
const SOCKS5_ADDR_TYPE_DOMAIN: u8 = 0x03;
const SOCKS5_ADDR_TYPE_IPV6: u8 = 0x04;
const SOCKS5_REPLY_SUCCEEDED: u8 = 0x00;
```

---

## Connection Pooling Implementation

Connection pooling is a mandatory feature that pre-establishes data channel connections to reduce latency and improve throughput.

### Connection Pool Architecture

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                           CONNECTION POOL ARCHITECTURE                         │
│                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────┐    │
│  │                         ChannelPool<T: Transport>                      │    │
│  │                                                                        │    │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐         │    │
│  │  │  TCP Channels   │  │  UDP Channels   │  │ Pending Creates │         │    │
│  │  │  (VecDeque)     │  │  (VecDeque)     │  │  (Semaphore)    │         │    │
│  │  │                 │  │                 │  │                 │         │    │
│  │  │  [Channel 1]    │  │  [Channel A]    │  │  permits: N     │         │    │
│  │  │  [Channel 2]    │  │  [Channel B]    │  │                 │         │    │
│  │  │  [Channel 3]    │  │  [Channel C]    │  │                 │         │    │
│  │  │  ...            │  │  ...            │  │                 │         │    │
│  │  └────────┬────────┘  └────────┬────────┘  └─────────────────┘         │    │
│  │           │                    │                                       │    │
│  │           ▼                    ▼                                       │    │
│  │  ┌─────────────────────────────────────────────────────────────┐       │    │
│  │  │                    Pool Manager Task                         │      │    │
│  │  │                                                              │      │    │
│  │  │  • Monitors pool size                                        │      │    │
│  │  │  • Pre-creates channels when below min_size                  │      │    │
│  │  │  • Validates channel health                                  │      │    │
│  │  │  • Removes stale connections                                 │      │    │
│  │  │  • Respects max_size limit                                   │      │    │
│  │  └──────────────────────────────────────────────────────────────┘      │    │
│  └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                │
│  Usage Flow:                                                                   │
│  ┌──────────┐    acquire()    ┌──────────┐    use channel    ┌──────────┐      │
│  │  Client  │ ───────────────►│   Pool   │ ────────────────► │  SOCKS5  │      │
│  │  Request │                 │          │                   │  Handler │      │
│  └──────────┘                 └──────────┘                   └──────────┘      │
│                                    ▲                              │            │
│                                    │         release()            │            │
│                                    └──────────────────────────────┘            │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘
```

### Pool Configuration

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PoolConfig {
    /// Minimum number of pre-established TCP channels
    #[serde(default = "default_min_tcp_channels")]
    pub min_tcp_channels: usize,

    /// Maximum number of TCP channels
    #[serde(default = "default_max_tcp_channels")]
    pub max_tcp_channels: usize,

    /// Minimum number of pre-established UDP channels
    #[serde(default = "default_min_udp_channels")]
    pub min_udp_channels: usize,

    /// Maximum number of UDP channels
    #[serde(default = "default_max_udp_channels")]
    pub max_udp_channels: usize,

    /// Channel idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,

    /// Health check interval in seconds
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,

    /// Maximum time to wait for a channel from the pool
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout: u64,
}

fn default_min_tcp_channels() -> usize { 2 }
fn default_max_tcp_channels() -> usize { 10 }
fn default_min_udp_channels() -> usize { 1 }
fn default_max_udp_channels() -> usize { 5 }
fn default_idle_timeout() -> u64 { 300 }
fn default_health_check_interval() -> u64 { 30 }
fn default_acquire_timeout() -> u64 { 10 }
```

### Channel Pool Implementation (`src/pool/channel_pool.rs`)

```rust
use crate::config::PoolConfig;
use crate::protocol::*;
use crate::transport::{AddrMaybeCached, SocketOpts, Transport};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, Notify, Semaphore};
use tracing::{debug, error, info, warn};

/// A pooled channel with metadata
struct PooledChannel<S> {
    stream: S,
    created_at: Instant,
    last_used: Instant,
}

impl<S> PooledChannel<S> {
    fn new(stream: S) -> Self {
        let now = Instant::now();
        PooledChannel {
            stream,
            created_at: now,
            last_used: now,
        }
    }

    fn is_stale(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }
}

/// Connection pool for data channels
pub struct ChannelPool<T: Transport> {
    config: PoolConfig,
    transport: Arc<T>,
    remote_addr: AddrMaybeCached,
    session_key: Digest,

    /// Available TCP channels
    tcp_channels: Mutex<VecDeque<PooledChannel<T::Stream>>>,

    /// Available UDP channels
    udp_channels: Mutex<VecDeque<PooledChannel<T::Stream>>>,

    /// Semaphore to limit concurrent channel creation
    create_semaphore: Semaphore,

    /// Notify when channels become available
    channel_available: Notify,

    /// Current number of active TCP channels (in use + pooled)
    active_tcp_count: AtomicUsize,

    /// Current number of active UDP channels (in use + pooled)
    active_udp_count: AtomicUsize,

    /// Shutdown signal
    shutdown: Notify,
}

impl<T: Transport + 'static> ChannelPool<T> {
    /// Create a new channel pool
    pub async fn new(
        config: PoolConfig,
        transport: Arc<T>,
        remote_addr: AddrMaybeCached,
        session_key: Digest,
    ) -> Result<Arc<Self>> {
        let pool = Arc::new(ChannelPool {
            config: config.clone(),
            transport,
            remote_addr,
            session_key,
            tcp_channels: Mutex::new(VecDeque::new()),
            udp_channels: Mutex::new(VecDeque::new()),
            create_semaphore: Semaphore::new(config.max_tcp_channels + config.max_udp_channels),
            channel_available: Notify::new(),
            active_tcp_count: AtomicUsize::new(0),
            active_udp_count: AtomicUsize::new(0),
            shutdown: Notify::new(),
        });

        // Pre-populate the pool
        pool.clone().warm_up().await?;

        // Start the pool manager task
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            pool_clone.run_manager().await;
        });

        Ok(pool)
    }

    /// Pre-populate the pool with minimum channels
    async fn warm_up(self: Arc<Self>) -> Result<()> {
        info!(
            "Warming up connection pool: {} TCP, {} UDP channels",
            self.config.min_tcp_channels,
            self.config.min_udp_channels
        );

        // Create minimum TCP channels
        let mut tasks = Vec::new();
        for _ in 0..self.config.min_tcp_channels {
            let pool = self.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = pool.create_tcp_channel().await {
                    warn!("Failed to pre-create TCP channel: {:?}", e);
                }
            }));
        }

        // Create minimum UDP channels
        for _ in 0..self.config.min_udp_channels {
            let pool = self.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = pool.create_udp_channel().await {
                    warn!("Failed to pre-create UDP channel: {:?}", e);
                }
            }));
        }

        // Wait for all warmup tasks
        for task in tasks {
            let _ = task.await;
        }

        info!("Connection pool warmed up successfully");
        Ok(())
    }

    /// Create a new TCP data channel and add to pool
    async fn create_tcp_channel(&self) -> Result<()> {
        let _permit = self.create_semaphore.acquire().await?;

        if self.active_tcp_count.load(Ordering::Relaxed) >= self.config.max_tcp_channels {
            return Ok(()); // At capacity
        }

        let stream = self.establish_data_channel(DataChannelCmd::StartForwardTcp).await?;

        let mut channels = self.tcp_channels.lock().await;
        channels.push_back(PooledChannel::new(stream));
        self.active_tcp_count.fetch_add(1, Ordering::Relaxed);

        self.channel_available.notify_one();
        debug!("Created new TCP channel, pool size: {}", channels.len());

        Ok(())
    }

    /// Create a new UDP data channel and add to pool
    async fn create_udp_channel(&self) -> Result<()> {
        let _permit = self.create_semaphore.acquire().await?;

        if self.active_udp_count.load(Ordering::Relaxed) >= self.config.max_udp_channels {
            return Ok(()); // At capacity
        }

        let stream = self.establish_data_channel(DataChannelCmd::StartForwardUdp).await?;

        let mut channels = self.udp_channels.lock().await;
        channels.push_back(PooledChannel::new(stream));
        self.active_udp_count.fetch_add(1, Ordering::Relaxed);

        self.channel_available.notify_one();
        debug!("Created new UDP channel, pool size: {}", channels.len());

        Ok(())
    }

    /// Establish a data channel with the server
    async fn establish_data_channel(&self, cmd: DataChannelCmd) -> Result<T::Stream> {
        let mut conn = self.transport
            .connect(&self.remote_addr)
            .await
            .context("Failed to connect to server")?;

        // Send data channel hello
        let hello = Hello::DataChannelHello(CURRENT_PROTO_VERSION, self.session_key);
        conn.write_all(&bincode::serialize(&hello)?).await?;
        conn.flush().await?;

        // Wait for command acknowledgment
        let received_cmd = read_data_cmd(&mut conn).await?;

        // Verify we got the expected command type
        match (&cmd, &received_cmd) {
            (DataChannelCmd::StartForwardTcp, DataChannelCmd::StartForwardTcp) => {}
            (DataChannelCmd::StartForwardUdp, DataChannelCmd::StartForwardUdp) => {}
            _ => anyhow::bail!("Unexpected data channel command: {:?}", received_cmd),
        }

        Ok(conn)
    }

    /// Acquire a TCP channel from the pool
    pub async fn acquire_tcp_channel(&self) -> Result<PooledChannelGuard<T::Stream>> {
        let timeout_duration = Duration::from_secs(self.config.acquire_timeout);

        let deadline = Instant::now() + timeout_duration;

        loop {
            // Try to get a channel from the pool
            {
                let mut channels = self.tcp_channels.lock().await;

                // Remove stale channels
                let idle_timeout = Duration::from_secs(self.config.idle_timeout);
                while let Some(front) = channels.front() {
                    if front.is_stale(idle_timeout) {
                        channels.pop_front();
                        self.active_tcp_count.fetch_sub(1, Ordering::Relaxed);
                        debug!("Removed stale TCP channel");
                    } else {
                        break;
                    }
                }

                if let Some(mut channel) = channels.pop_front() {
                    channel.last_used = Instant::now();
                    return Ok(PooledChannelGuard {
                        stream: Some(channel.stream),
                        pool: self,
                        is_tcp: true,
                    });
                }
            }

            // No channel available, try to create one
            if self.active_tcp_count.load(Ordering::Relaxed) < self.config.max_tcp_channels {
                if let Err(e) = self.create_tcp_channel().await {
                    warn!("Failed to create TCP channel on demand: {:?}", e);
                }
                continue; // Try again
            }

            // At capacity, wait for a channel to become available
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("Timeout waiting for TCP channel");
            }

            tokio::select! {
                _ = self.channel_available.notified() => continue,
                _ = tokio::time::sleep(remaining) => {
                    anyhow::bail!("Timeout waiting for TCP channel");
                }
            }
        }
    }

    /// Acquire a UDP channel from the pool
    pub async fn acquire_udp_channel(&self) -> Result<PooledChannelGuard<T::Stream>> {
        let timeout_duration = Duration::from_secs(self.config.acquire_timeout);
        let deadline = Instant::now() + timeout_duration;

        loop {
            {
                let mut channels = self.udp_channels.lock().await;

                // Remove stale channels
                let idle_timeout = Duration::from_secs(self.config.idle_timeout);
                while let Some(front) = channels.front() {
                    if front.is_stale(idle_timeout) {
                        channels.pop_front();
                        self.active_udp_count.fetch_sub(1, Ordering::Relaxed);
                        debug!("Removed stale UDP channel");
                    } else {
                        break;
                    }
                }

                if let Some(mut channel) = channels.pop_front() {
                    channel.last_used = Instant::now();
                    return Ok(PooledChannelGuard {
                        stream: Some(channel.stream),
                        pool: self,
                        is_tcp: false,
                    });
                }
            }

            if self.active_udp_count.load(Ordering::Relaxed) < self.config.max_udp_channels {
                if let Err(e) = self.create_udp_channel().await {
                    warn!("Failed to create UDP channel on demand: {:?}", e);
                }
                continue;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("Timeout waiting for UDP channel");
            }

            tokio::select! {
                _ = self.channel_available.notified() => continue,
                _ = tokio::time::sleep(remaining) => {
                    anyhow::bail!("Timeout waiting for UDP channel");
                }
            }
        }
    }

    /// Return a channel to the pool (called by PooledChannelGuard on drop)
    fn return_channel(&self, stream: T::Stream, is_tcp: bool) {
        let pool = self.clone();
        tokio::spawn(async move {
            if is_tcp {
                let mut channels = pool.tcp_channels.lock().await;
                if channels.len() < pool.config.max_tcp_channels {
                    channels.push_back(PooledChannel::new(stream));
                    pool.channel_available.notify_one();
                } else {
                    pool.active_tcp_count.fetch_sub(1, Ordering::Relaxed);
                }
            } else {
                let mut channels = pool.udp_channels.lock().await;
                if channels.len() < pool.config.max_udp_channels {
                    channels.push_back(PooledChannel::new(stream));
                    pool.channel_available.notify_one();
                } else {
                    pool.active_udp_count.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });
    }

    /// Run the pool manager (background task)
    async fn run_manager(self: Arc<Self>) {
        let health_interval = Duration::from_secs(self.config.health_check_interval);

        loop {
            tokio::select! {
                _ = self.shutdown.notified() => {
                    info!("Pool manager shutting down");
                    break;
                }
                _ = tokio::time::sleep(health_interval) => {
                    self.maintain_pool().await;
                }
            }
        }
    }

    /// Maintain pool health and size
    async fn maintain_pool(&self) {
        // Ensure minimum TCP channels
        let tcp_count = {
            let channels = self.tcp_channels.lock().await;
            channels.len()
        };

        if tcp_count < self.config.min_tcp_channels {
            let needed = self.config.min_tcp_channels - tcp_count;
            for _ in 0..needed {
                if let Err(e) = self.create_tcp_channel().await {
                    warn!("Failed to replenish TCP channel: {:?}", e);
                }
            }
        }

        // Ensure minimum UDP channels
        let udp_count = {
            let channels = self.udp_channels.lock().await;
            channels.len()
        };

        if udp_count < self.config.min_udp_channels {
            let needed = self.config.min_udp_channels - udp_count;
            for _ in 0..needed {
                if let Err(e) = self.create_udp_channel().await {
                    warn!("Failed to replenish UDP channel: {:?}", e);
                }
            }
        }

        debug!(
            "Pool health check: TCP={}/{}, UDP={}/{}",
            tcp_count, self.config.min_tcp_channels,
            udp_count, self.config.min_udp_channels
        );
    }

    /// Shutdown the pool
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

/// RAII guard that returns the channel to the pool on drop
pub struct PooledChannelGuard<'a, S> {
    stream: Option<S>,
    pool: &'a ChannelPool<dyn Transport<Stream = S>>,
    is_tcp: bool,
}

impl<'a, S> PooledChannelGuard<'a, S> {
    /// Take ownership of the stream (won't return to pool)
    pub fn take(mut self) -> S {
        self.stream.take().unwrap()
    }

    /// Get a mutable reference to the stream
    pub fn stream_mut(&mut self) -> &mut S {
        self.stream.as_mut().unwrap()
    }
}

impl<'a, S> Drop for PooledChannelGuard<'a, S> {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            // Return stream to pool
            // Note: In actual implementation, we'd need to handle this differently
            // since we can't call async from drop. Using a channel or spawning works.
        }
    }
}

impl<'a, S> std::ops::Deref for PooledChannelGuard<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        self.stream.as_ref().unwrap()
    }
}

impl<'a, S> std::ops::DerefMut for PooledChannelGuard<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.stream.as_mut().unwrap()
    }
}
```

### Updated Client with Connection Pool (`src/client.rs` changes)

```rust
// In the ControlChannel implementation, integrate the pool:

impl<T: 'static + Transport> ControlChannel<T> {
    async fn run_once(&self) -> Result<()> {
        let mut remote_addr = AddrMaybeCached::new(&self.config.remote_addr);
        remote_addr.resolve().await?;

        let mut conn = self.transport.connect(&remote_addr).await?;
        T::hint(&conn, SocketOpts::for_control_channel());

        // Perform handshake
        let session_key = self.do_handshake(&mut conn).await?;

        info!("Control channel established");

        // Initialize connection pool
        let pool = ChannelPool::new(
            self.config.pool.clone(),
            self.transport.clone(),
            remote_addr.clone(),
            session_key,
        ).await?;

        info!("Connection pool initialized");

        // Listen for commands
        loop {
            tokio::select! {
                cmd = read_control_cmd(&mut conn) => {
                    match cmd? {
                        ControlChannelCmd::CreateDataChannel => {
                            // Use pooled channel instead of creating new one
                            let pool_clone = pool.clone();
                            let socks_config = self.config.socks.clone();

                            tokio::spawn(async move {
                                if let Err(e) = handle_pooled_request(pool_clone, socks_config).await {
                                    warn!("Request handling error: {:?}", e);
                                }
                            });
                        }
                        ControlChannelCmd::HeartBeat => {
                            debug!("Heartbeat received");
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(self.config.heartbeat_timeout)) => {
                    pool.shutdown();
                    bail!("Heartbeat timeout");
                }
            }
        }
    }
}

async fn handle_pooled_request<T: Transport>(
    pool: Arc<ChannelPool<T>>,
    socks_config: SocksConfig,
) -> Result<()> {
    // Acquire a channel from the pool
    let mut channel = pool.acquire_tcp_channel().await?;

    // Process SOCKS5 on the pooled channel
    // Note: We take ownership since SOCKS5 handling is a long-lived operation
    let stream = channel.take();

    handle_socks5_on_stream(stream, &socks_config).await
}
```

---

## Security Considerations

### General Security
1. **Token Authentication**: The rathole protocol requires a shared token for client authentication
2. **Transport Encryption**: Use TLS or Noise transport for encrypted tunnel
3. **No Local Binding**: Services never bind locally, reducing attack surface
4. **Service Isolation**: Each service (SOCKS5, SSH) runs independently

### SOCKS5 Security
5. **SOCKS5 Authentication**: Optional username/password auth for SOCKS5 layer
6. **Command Restrictions**: Optionally disable UDP ASSOCIATE or BIND commands
7. **DNS Privacy**: DNS resolution can be performed on client side

### SSH Security
8. **Host Key Management**: Generate and securely store host keys
9. **Public Key Authentication**: Preferred over password authentication
10. **Password Hashing**: Passwords should be hashed (argon2) in production configs
11. **Max Auth Attempts**: Configurable limit to prevent brute force
12. **User Isolation**: Per-user home directories and shell configuration
13. **Subsystem Control**: Optionally disable shell, exec, or SFTP
14. **Session Auditing**: Log all SSH authentication and command execution

---

## Testing Strategy

### Unit Tests
1. **SOCKS5 Protocol**: Test SOCKS5 parsing, auth, and command handling
2. **SSH Protocol**: Test SSH auth verification, key parsing
3. **Configuration**: Test config loading and validation
4. **Pool Logic**: Test connection pool management

### Integration Tests
5. **Full SOCKS5 Flow**: Test with mock rathole server
6. **Full SSH Flow**: Test SSH session lifecycle
7. **Multi-Service**: Test SOCKS5 and SSH running simultaneously

### Manual Testing

```bash
# Start rathole server (with server config)
rathole server.toml

# Start SocksRat client
socksrat -c client.toml

# =====================
# Test SOCKS5 Service
# =====================

# Test SOCKS5 proxy (from server side)
curl -x socks5://localhost:1080 https://example.com

# Test with authentication
curl -x socks5://user:pass@localhost:1080 https://example.com

# Test UDP (DNS query through SOCKS5)
dig @8.8.8.8 example.com +tcp  # via TCP
# For UDP testing, use a SOCKS5-aware DNS tool

# =====================
# Test SSH Service
# =====================

# Test SSH connection (from server side, connects to port 2222)
ssh -p 2222 admin@localhost

# Test with specific key
ssh -p 2222 -i ~/.ssh/id_rsa admin@localhost

# Test exec command
ssh -p 2222 admin@localhost "whoami"

# Test SFTP
sftp -P 2222 admin@localhost

# Test SCP
scp -P 2222 localfile.txt admin@localhost:/tmp/
```

---

## Future Enhancements

1. **Metrics**: Prometheus metrics for monitoring (connections, bandwidth, errors)
2. **Hot Reload**: Configuration hot reload support
3. **Access Control Lists**: IP-based allow/deny lists for services
4. **Rate Limiting**: Connection rate limiting per client
5. **SSH Agent Forwarding**: Support for SSH agent protocol
6. **SSH Port Forwarding**: Support for -L and -R style forwarding within SSH sessions
7. **SOCKS5 BIND**: Full BIND command support for incoming connections
