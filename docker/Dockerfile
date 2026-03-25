# Stage 1: Build
FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY shared/ shared/
COPY server/ server/
COPY client/ client/

# Build only the server binary in release mode
RUN cargo build --release -p server

# Stage 2: Runtime — minimal image, binary only
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/server /usr/local/bin/sots-server

EXPOSE 7777/udp

ENTRYPOINT ["sots-server"]
