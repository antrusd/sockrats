.PHONY: all build test coverage lint clean fmt check \
        build-linux build-linux-docker \
		build-windows build-windows-docker build-macos-docker \
		build-all build-all-docker

# Docker image for builds - use rust:1.93.0-alpine3.23 for static builds with musl
DOCKER_IMAGE := rust:1.93.0-alpine3.23
DOCKER_IMAGE_OSXCROSS := rust:slim-trixie
APP_NAME := sockrats

# Target architectures (musl for static linking)
LINUX_TARGETS := x86_64-unknown-linux-musl aarch64-unknown-linux-musl
WINDOWS_TARGETS := x86_64-pc-windows-gnu
MACOS_TARGETS := x86_64-apple-darwin aarch64-apple-darwin

all: lint test build

# Build in Docker (static with musl)
build:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && cargo build --release"

# Check in Docker
check:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && cargo check --all-features"

# Run tests
test:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && cargo test --all-features"

# Run coverage
coverage:
	docker run --network host --rm --privileged --security-opt seccomp=unconfined \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && \
		       cargo install cargo-tarpaulin && \
		       cargo tarpaulin --out Html --fail-under 80"

# Lint
lint:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && rustup component add rustfmt clippy && cargo fmt -- --check && cargo clippy --all-features -- -D warnings"

# Format code
fmt:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && rustup component add rustfmt && cargo fmt"

# Clean build artifacts
clean:
	type cargo >/dev/null && cargo clean || docker run --rm \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) cargo clean
	rm -rf target/
	rm -rf dist/

# Run the application locally
run:
	cargo run -- -c examples/config.toml

# Watch for changes and run tests
watch:
	cargo watch -x "test" -x "run -- -c examples/config.toml"

# ============================================
# Cross-compilation targets using Docker
# All use rust:1.93.0-alpine3.23 for static builds
# ============================================

# Build for Linux x86_64 (static with musl - native in Alpine)
build-linux-x86_64-docker:
	@echo "Building for x86_64-unknown-linux-musl (static)..."
	@mkdir -p dist/x86_64-unknown-linux-musl
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c 'apk add --no-cache musl-dev pkgconf cmake make perl clang openssl-dev openssl-libs-static && \
			cargo build --release && \
			cp target/release/$(APP_NAME) /app/dist/x86_64-unknown-linux-musl/'
	@echo "Built: dist/x86_64-unknown-linux-musl/$(APP_NAME) (static)"

# Build for Linux ARM64 (static with musl) using cargo-zigbuild
build-linux-aarch64-docker:
	@echo "Building for aarch64-unknown-linux-musl (static)..."
	@mkdir -p dist/aarch64-unknown-linux-musl
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c 'set -e && \
			apk add --no-cache musl-dev pkgconf py3-pip cmake make perl clang && \
			pip3 install --break-system-packages ziglang && \
			cargo install cargo-zigbuild && \
			rustup target add aarch64-unknown-linux-musl && \
			cargo zigbuild --release --target aarch64-unknown-linux-musl && \
			cp target/aarch64-unknown-linux-musl/release/$(APP_NAME) /app/dist/aarch64-unknown-linux-musl/'
	@echo "Built: dist/aarch64-unknown-linux-musl/$(APP_NAME) (static)"

# Build all Linux targets in Docker (all static builds)
build-linux-docker: build-linux-x86_64-docker build-linux-aarch64-docker
	@echo "All Linux builds completed (static)!"

# Build for Windows x86_64 (static) using cargo-zigbuild
build-windows-docker:
	@echo "Building for x86_64-pc-windows-gnu (static)..."
	@mkdir -p dist/x86_64-pc-windows-gnu
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c 'set -e && \
			apk add --no-cache musl-dev pkgconf py3-pip cmake make perl clang && \
			pip3 install --break-system-packages ziglang && \
			cargo install cargo-zigbuild && \
			rustup target add x86_64-pc-windows-gnu && \
			cargo zigbuild --release --target x86_64-pc-windows-gnu && \
			cp target/x86_64-pc-windows-gnu/release/$(APP_NAME).exe /app/dist/x86_64-pc-windows-gnu/'
	@echo "Built: dist/x86_64-pc-windows-gnu/$(APP_NAME).exe (static)"

# Build for macOS x86_64 (Intel) using osxcross in rust:slim-trixie
build-macos-x86_64-docker:
	@echo "Building for x86_64-apple-darwin..."
	@mkdir -p dist/x86_64-apple-darwin
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_OSXCROSS) \
		sh -c 'set -e && \
			apt-get update && apt-get install -y pkg-config clang cmake git libxml2-dev libz-dev curl xz-utils libbz2-dev libssl-dev libxar-dev liblzma-dev musl-tools && \
			rustup target add x86_64-apple-darwin && \
			if [ ! -d /opt/osxcross ]; then \
				git clone https://github.com/tpoechtrager/osxcross /tmp/osxcross && \
				(cd /tmp/osxcross && \
				curl -L -o tarballs/MacOSX14.0.sdk.tar.xz "https://github.com/joseluisq/macosx-sdks/releases/download/14.0/MacOSX14.0.sdk.tar.xz" && \
				UNATTENDED=1 ./build.sh && \
				mv target /opt/osxcross); \
			fi && \
			export PATH="/opt/osxcross/bin:$$PATH" && \
			export LD_LIBRARY_PATH="/opt/osxcross/lib:$$LD_LIBRARY_PATH" && \
			export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=x86_64-apple-darwin23-clang && \
			export CC_x86_64_apple_darwin=x86_64-apple-darwin23-clang && \
			export CXX_x86_64_apple_darwin=x86_64-apple-darwin23-clang++ && \
			export AR_x86_64_apple_darwin=x86_64-apple-darwin23-ar && \
			cargo build --release --target x86_64-apple-darwin && \
			cp target/x86_64-apple-darwin/release/$(APP_NAME) /app/dist/x86_64-apple-darwin/'
	@echo "Built: dist/x86_64-apple-darwin/$(APP_NAME)"

# Build for macOS ARM64 (M1/M2/M3) using osxcross in rust:slim-trixie
build-macos-aarch64-docker:
	@echo "Building for aarch64-apple-darwin (Apple Silicon M1/M2/M3)..."
	@mkdir -p dist/aarch64-apple-darwin
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_OSXCROSS) \
		sh -c 'set -e && \
			apt-get update && apt-get install -y pkg-config clang cmake git libxml2-dev libz-dev curl xz-utils libbz2-dev libssl-dev libxar-dev liblzma-dev musl-tools && \
			rustup target add aarch64-apple-darwin && \
			if [ ! -d /opt/osxcross ]; then \
				git clone https://github.com/tpoechtrager/osxcross /tmp/osxcross && \
				(cd /tmp/osxcross && \
				curl -L -o tarballs/MacOSX14.0.sdk.tar.xz "https://github.com/joseluisq/macosx-sdks/releases/download/14.0/MacOSX14.0.sdk.tar.xz" && \
				UNATTENDED=1 ./build.sh && \
				mv target /opt/osxcross); \
			fi && \
			export PATH="/opt/osxcross/bin:$$PATH" && \
			export LD_LIBRARY_PATH="/opt/osxcross/lib:$$LD_LIBRARY_PATH" && \
			export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=aarch64-apple-darwin23-clang && \
			export CC_aarch64_apple_darwin=aarch64-apple-darwin23-clang && \
			export CXX_aarch64_apple_darwin=aarch64-apple-darwin23-clang++ && \
			export AR_aarch64_apple_darwin=aarch64-apple-darwin23-ar && \
			cargo build --release --target aarch64-apple-darwin && \
			cp target/aarch64-apple-darwin/release/$(APP_NAME) /app/dist/aarch64-apple-darwin/'
	@echo "Built: dist/aarch64-apple-darwin/$(APP_NAME)"

# Build all macOS targets in Docker
build-macos-docker: build-macos-x86_64-docker build-macos-aarch64-docker
	@echo "All macOS builds completed!"

# Build all platforms in Docker
build-all-docker: build-linux-docker build-windows-docker build-macos-docker
	@echo "All cross-platform builds completed!"
	@echo "Binaries are in dist/"

# Build for a specific target in Docker
build-target-docker:
	@if [ -z "$(TARGET)" ]; then \
		echo "Usage: make build-target-docker TARGET=<target>"; \
		echo ""; \
		echo "Available targets:"; \
		echo "  Linux: $(LINUX_TARGETS)"; \
		echo "  Windows: $(WINDOWS_TARGETS)"; \
		echo "  macOS: $(MACOS_TARGETS)"; \
		exit 1; \
	fi
	@echo "Building for $(TARGET)..."
	@if echo "$(TARGET)" | grep -q "x86_64.*linux-musl"; then \
		$(MAKE) build-linux-x86_64-docker; \
	elif echo "$(TARGET)" | grep -q "aarch64.*linux-musl"; then \
		$(MAKE) build-linux-aarch64-docker; \
	elif echo "$(TARGET)" | grep -q "windows"; then \
		$(MAKE) build-windows-docker; \
	elif echo "$(TARGET)" | grep -q "x86_64.*darwin"; then \
		$(MAKE) build-macos-x86_64-docker; \
	elif echo "$(TARGET)" | grep -q "aarch64.*darwin"; then \
		$(MAKE) build-macos-aarch64-docker; \
	else \
		echo "Unknown target: $(TARGET)"; \
		exit 1; \
	fi

# Create release archives
release-archives:
	@mkdir -p dist/release
	@for target in $(LINUX_TARGETS); do \
		if [ -f "dist/$$target/$(APP_NAME)" ]; then \
			tar -czvf "dist/release/$(APP_NAME)-$$target.tar.gz" -C "dist/$$target" $(APP_NAME); \
			echo "Created: dist/release/$(APP_NAME)-$$target.tar.gz"; \
		fi; \
	done
	@for target in $(WINDOWS_TARGETS); do \
		if [ -f "dist/$$target/$(APP_NAME).exe" ]; then \
			cd dist/$$target && zip -j "../release/$(APP_NAME)-$$target.zip" $(APP_NAME).exe && cd ../..; \
			echo "Created: dist/release/$(APP_NAME)-$$target.zip"; \
		fi; \
	done
	@for target in $(MACOS_TARGETS); do \
		if [ -f "dist/$$target/$(APP_NAME)" ]; then \
			tar -czvf "dist/release/$(APP_NAME)-$$target.tar.gz" -C "dist/$$target" $(APP_NAME); \
			echo "Created: dist/release/$(APP_NAME)-$$target.tar.gz"; \
		fi; \
	done

# Show available cross-compilation targets
targets:
	@echo "Available cross-compilation targets:"
	@echo ""
	@echo "Linux (static with musl):"
	@for target in $(LINUX_TARGETS); do echo "  - $$target"; done
	@echo ""
	@echo "Windows (static):"
	@for target in $(WINDOWS_TARGETS); do echo "  - $$target"; done
	@echo ""
	@echo "macOS:"
	@for target in $(MACOS_TARGETS); do echo "  - $$target"; done
	@echo ""
	@echo "Usage:"
	@echo "  make build-linux-docker        # Build for all Linux targets (static, Alpine)"
	@echo "  make build-windows-docker      # Build for Windows (Alpine + zigbuild)"
	@echo "  make build-macos-docker        # Build for macOS (Debian + osxcross)"
	@echo "  make build-all-docker          # Build all platforms"
	@echo "  make build-target-docker TARGET=x86_64-unknown-linux-musl"

# Help
help:
	@echo "Sockrats Build System"
	@echo ""
	@echo "Basic targets:"
	@echo "  make build             - Build for current platform"
	@echo "  make test              - Run tests"
	@echo "  make lint              - Run linter"
	@echo "  make fmt               - Format code"
	@echo "  make clean             - Clean build artifacts"
	@echo ""
	@echo "Docker targets:"
	@echo "  make build-docker      - Build in Docker container (static)"
	@echo "  make test-docker       - Run tests in Docker"
	@echo ""
	@echo "Cross-compilation targets:"
	@echo "  make build-linux-docker        - Build for Linux (x86_64, ARM64) - static musl"
	@echo "  make build-windows-docker      - Build for Windows x86_64 - static"
	@echo "  make build-macos-docker        - Build for macOS (Intel + M1/M2/M3) - osxcross"
	@echo "  make build-all-docker          - Build all platforms"
	@echo "  make build-target-docker TARGET=<target>"
	@echo "  make targets                   - Show available targets"
	@echo "  make release-archives          - Create release archives"
