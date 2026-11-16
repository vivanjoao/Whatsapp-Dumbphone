# syntax=docker/dockerfile:1
FROM rust:1.99.1-slim-bookworm AS builder
# Bind sources read-only and use a single build layer to avoid copying the Rust
# toolchain/target repeatedly on filesystems whose Docker driver is vfs.
RUN --mount=type=bind,source=.,target=/src \
    --mount=type=secret,id=proxy_ca \
    set -eu; \
    apt-get update; \
    apt-get install -y --no-install-recommends build-essential git pkg-config ca-certificates; \
    if [ -f /run/secrets/proxy_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/proxy_ca; fi; \
    cargo build --manifest-path /src/Cargo.toml --target-dir /tmp/target --release --locked -j 2 -p dumbphone-gateway; \
    cp /tmp/target/release/dumbphone-gateway /gateway; \
    rm -rf /tmp/target /usr/local/cargo/registry /usr/local/cargo/git /var/lib/apt/lists/*

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0700 /data
COPY --from=builder /gateway /usr/local/bin/gateway
ENV DATA_DIR=/data BIND_ADDR=0.0.0.0:3000
USER 10001:10001
WORKDIR /data
VOLUME ["/data"]
EXPOSE 3000
ENTRYPOINT ["gateway"]
