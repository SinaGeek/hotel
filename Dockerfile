# HMS server image (README §17). Build from the repository root:
#   docker build -f Dockerfile -t hms-server .
#
# Rustls is used for TLS inside the process, so the runtime image needs no
# OpenSSL libraries. Migrations are embedded at compile time by
# `sqlx::migrate!("../../migrations")`, so the whole workspace is copied in.

# ---------- build ----------
FROM rust:1.83-bookworm AS build
WORKDIR /src

# Cache dependency compilation before copying the sources.
COPY Cargo.toml Cargo.lock ./
COPY crates/hms-core/Cargo.toml crates/hms-core/Cargo.toml
COPY crates/hms-server/Cargo.toml crates/hms-server/Cargo.toml
COPY crates/hms-sync/Cargo.toml crates/hms-sync/Cargo.toml
COPY crates/hms-tests/Cargo.toml crates/hms-tests/Cargo.toml
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
RUN mkdir -p crates/hms-core/src crates/hms-server/src crates/hms-sync/src crates/hms-tests/src apps/desktop/src-tauri/src \
    && echo 'fn main() {}' > crates/hms-server/src/main.rs \
    && touch crates/hms-core/src/lib.rs crates/hms-sync/src/lib.rs crates/hms-tests/src/lib.rs apps/desktop/src-tauri/src/main.rs \
    && cargo build --release -p hms-server || true

COPY . .
# `touch` so the real sources are newer than the dummy ones cached above.
RUN touch crates/hms-core/src/lib.rs crates/hms-server/src/main.rs \
    && cargo build --release -p hms-server

# ---------- runtime ----------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home hms

COPY --from=build /src/target/release/hms-server /usr/local/bin/hms-server

USER hms
ENV APP_ENV=production \
    RUST_LOG=info
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/hms-server"]
