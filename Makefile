.PHONY: all test coverage lint clean fmt check \
        build-linux-docker build-windows-docker build-macos-docker

export DOCKER_HOST := unix:///run/user/1000/docker.sock

# Docker images:
# - rust:1.93.0-alpine3.23 for standard non-cross builds (native musl)
# - ghcr.io/rust-cross/cargo-zigbuild:0.21.4 for all cross-compilation targets
DOCKER_IMAGE := rust:1.93.0-alpine3.23
DOCKER_IMAGE_ZIGBUILD := ghcr.io/rust-cross/cargo-zigbuild:0.21.4
APP_NAME := sockrats

# All targets - cross-compilation targets use zigbuild
LINUX_TARGETS := x86_64-unknown-linux-musl aarch64-unknown-linux-musl
WINDOWS_TARGETS := x86_64-pc-windows-gnu
MACOS_TARGETS := x86_64-apple-darwin aarch64-apple-darwin
ALL_TARGETS := $(LINUX_TARGETS) $(WINDOWS_TARGETS) $(MACOS_TARGETS)

all: lint test

# Check in Docker
check:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		cargo check --all-features

# Run tests
test:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		cargo test --all-features

# Run coverage
coverage:
	docker run --network host --rm --privileged --security-opt seccomp=unconfined \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "cargo install cargo-tarpaulin && cargo tarpaulin --out Html --fail-under 80"

# Lint
lint:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "rustup component add rustfmt clippy && cargo fmt -- --check && cargo clippy --all-features -- -D warnings"

# Format code
fmt:
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) \
		sh -c "rustup component add rustfmt && cargo fmt"

# Clean build artifacts
clean:
	type cargo >/dev/null && cargo clean || docker run --rm \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) cargo clean
	rm -rf target/
	rm -rf dist/

# ============================================
# Cross-compilation targets using cargo-zigbuild
# All cross builds use ghcr.io/rust-cross/cargo-zigbuild:0.21.4
# No OpenSSL, no osxcross — pure Rust deps + zig cross-compiler
# ============================================

# Build for Linux x86_64 (static with musl)
build-linux-x86_64-docker:
	@echo "Building for x86_64-unknown-linux-musl (static)..."
	@mkdir -p dist/x86_64-unknown-linux-musl
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_ZIGBUILD) \
		sh -c 'set -e && \
			rustup target add x86_64-unknown-linux-musl && \
			cargo zigbuild --release --target x86_64-unknown-linux-musl && \
			cp target/x86_64-unknown-linux-musl/release/$(APP_NAME) /app/dist/x86_64-unknown-linux-musl/'
	@echo "Built: dist/x86_64-unknown-linux-musl/$(APP_NAME) (static)"

# Build for Linux ARM64 (static with musl)
build-linux-aarch64-docker:
	@echo "Building for aarch64-unknown-linux-musl (static)..."
	@mkdir -p dist/aarch64-unknown-linux-musl
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_ZIGBUILD) \
		sh -c 'set -e && \
			rustup target add aarch64-unknown-linux-musl && \
			cargo zigbuild --release --target aarch64-unknown-linux-musl && \
			cp target/aarch64-unknown-linux-musl/release/$(APP_NAME) /app/dist/aarch64-unknown-linux-musl/'
	@echo "Built: dist/aarch64-unknown-linux-musl/$(APP_NAME) (static)"

# Build all Linux targets
build-linux-docker: build-linux-x86_64-docker build-linux-aarch64-docker
	@echo "All Linux builds completed (static)!"

# Build for Windows x86_64 (static)
build-windows-x86_64-docker:
	@echo "Building for x86_64-pc-windows-gnu (static)..."
	@mkdir -p dist/x86_64-pc-windows-gnu
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_ZIGBUILD) \
		sh -c 'set -e && \
			rustup target add x86_64-pc-windows-gnu && \
			cargo zigbuild --release --target x86_64-pc-windows-gnu && \
			cp target/x86_64-pc-windows-gnu/release/$(APP_NAME).exe /app/dist/x86_64-pc-windows-gnu/'
	@echo "Built: dist/x86_64-pc-windows-gnu/$(APP_NAME).exe (static)"

# Build all Windows targets
build-windows-docker: build-windows-x86_64-docker
	@echo "All Windows builds completed (static)!"

# Build for macOS x86_64 (Intel)
build-macos-x86_64-docker:
	@echo "Building for x86_64-apple-darwin..."
	@mkdir -p dist/x86_64-apple-darwin
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_ZIGBUILD) \
		sh -c 'set -e && \
			rustup target add x86_64-apple-darwin && \
			cargo zigbuild --release --target x86_64-apple-darwin && \
			cp target/x86_64-apple-darwin/release/$(APP_NAME) /app/dist/x86_64-apple-darwin/'
	@echo "Built: dist/x86_64-apple-darwin/$(APP_NAME)"

# Build for macOS ARM64 (Apple Silicon M1/M2/M3)
build-macos-aarch64-docker:
	@echo "Building for aarch64-apple-darwin (Apple Silicon M1/M2/M3)..."
	@mkdir -p dist/aarch64-apple-darwin
	docker run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE_ZIGBUILD) \
		sh -c 'set -e && \
			rustup target add aarch64-apple-darwin && \
			cargo zigbuild --release --target aarch64-apple-darwin && \
			cp target/aarch64-apple-darwin/release/$(APP_NAME) /app/dist/aarch64-apple-darwin/'
	@echo "Built: dist/aarch64-apple-darwin/$(APP_NAME)"

# Build all macOS targets
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
	elif echo "$(TARGET)" | grep -q "x86_64.*windows"; then \
		$(MAKE) build-windows-x86_64-docker; \
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
	@echo "Available cross-compilation targets (all via zigbuild):"
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
	@echo "  make build-linux-docker        # Build for all Linux targets"
	@echo "  make build-windows-docker      # Build for all Windows targets"
	@echo "  make build-macos-docker        # Build for all macOS targets"
	@echo "  make build-all-docker          # Build all platforms"
	@echo "  make build-target-docker TARGET=x86_64-unknown-linux-musl"

# Help
help:
	@echo "Sockrats Build System"
	@echo ""
	@echo "Basic targets:"
	@echo "  make test              - Run tests"
	@echo "  make lint              - Run linter"
	@echo "  make fmt               - Format code"
	@echo "  make clean             - Clean build artifacts"
	@echo ""
	@echo "Cross-compilation targets (all use cargo-zigbuild):"
	@echo "  make build-linux-docker        - Build for Linux (x86_64, ARM64) - static musl"
	@echo "  make build-windows-docker      - Build for Windows (x86_64) - static"
	@echo "  make build-macos-docker        - Build for macOS (Intel + Apple Silicon)"
	@echo "  make build-all-docker          - Build all platforms"
	@echo "  make build-target-docker TARGET=<target>"
	@echo "  make targets                   - Show available targets"
	@echo "  make release-archives          - Create release archives"
