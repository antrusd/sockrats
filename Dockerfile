FROM ghcr.io/rust-cross/cargo-zigbuild:0.21.4

RUN cargo install cargo-tarpaulin && \
    rustup component add rustfmt clippy && \
    rustup target add x86_64-unknown-linux-musl && \
    rustup target add aarch64-unknown-linux-musl && \
    rustup target add x86_64-pc-windows-gnu && \
    rustup target add x86_64-apple-darwin && \
    rustup target add aarch64-apple-darwin
