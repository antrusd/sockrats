.PHONY: all test coverage lint clean fmt check test-no-default-features

APP_NAME := sockrats

DOCKER_HOST := unix:///run/user/$(shell id -u)/docker.sock
DOCKER_IMAGE := cargo-zigbuild:0.21.5

# Run build
build:
	cargo build

# Run release build
release:
	cargo build --release

# Run cross build
cross:
	cargo zigbuild --release --target $(TARGET)

# Run check
check:
	cargo check --all-features

# Run tests
test:
	cargo test --all-features

doc:
	cargo doc --features vncserver -p rfb-encodings --no-deps

# Build and test with no default features
test-no-default-features:
	cargo check --no-default-features
	cargo test --no-default-features

# Run coverage
coverage:
	cargo tarpaulin --out Html --fail-under 80

# Lint
lint:
	cargo fmt -- --check
	cargo clippy --all-features -- -D warnings

# Format code
fmt:
	cargo fmt

# Clean build artifacts
clean:
	cargo clean
	rm -rf target/
	rm -rf dist/

# Cross build in Docker
cross-in-docker:
	docker --host $(DOCKER_HOST) run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) make TARGET=$(TARGET) release

# Run individual targets in Docker
%-in-docker:
	docker --host $(DOCKER_HOST) run --network host --rm \
		-v $${HOME}/Workspaces/cargo/registry:/usr/local/cargo/registry \
		-v "$$(pwd)":/app -w /app $(DOCKER_IMAGE) make $*
