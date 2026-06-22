# Shared multi-stage build for the Geos Rust services (geos-api, geos-workers).
# Select the binary with `--build-arg BIN=geos-api` (see docker-compose.yml).
FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ARG BIN=geos-api
RUN cargo build --release --locked --bin ${BIN}

FROM debian:bookworm-slim AS runtime
ARG BIN=geos-api
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/${BIN} /usr/local/bin/geos-service
ENTRYPOINT ["/usr/local/bin/geos-service"]
