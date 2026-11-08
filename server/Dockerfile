# HMS server image (README ~17). Build from server/ (the server workspace is
# self-contained — see docs/restructure.md):
#   docker build -t hms-server .
#
# The server speaks plain HTTP on :8080 and must sit behind a TLS-terminating
# reverse proxy (server/deploy/Caddyfile or server/deploy/nginx.conf).
# Migrations are embedded at compile time by `sqlx::migrate!("../../migrations")`,
# so the whole server workspace is copied in.

# ---------- build ----------
FROM rust:1.83-bookworm AS build
WORKDIR /src

# Dependency layer: copy every workspace member's manifest plus a stub source
# for each, so cargo can resolve and compile third-party crates once and reuse
# that layer while only application code changes.
COPY Cargo.toml Cargo.lock ./
COPY crates/hms-core/Cargo.toml crates/hms-core/Cargo.toml
COPY crates/hms-server/Cargo.toml crates/hms-server/Cargo.toml
COPY crates/hms-lan-hub/Cargo.toml crates/hms-lan-hub/Cargo.toml
COPY crates/hms-tests/Cargo.toml crates/hms-tests/Cargo.toml
COPY migrations migrations
RUN set -eux; \
    for d in crates/hms-core crates/hms-server crates/hms-lan-hub crates/hms-tests; do \
        mkdir -p "$d/src"; done; \
    for lib in crates/hms-core crates/hms-tests; do \
        : > "$lib/src/lib.rs"; done; \
    echo 'mod api;' > crates/hms-server/src/lib_stub.rs && : > crates/hms-server/src/api_stub.rs; \
    echo 'fn main() {}' > crates/hms-server/src/main.rs; \
    echo 'fn main() {}' > crates/hms-lan-hub/src/main.rs; \
    # `|| true`: the stubs are not the real crates; the goal is the registry +
    # dependency compile cache, not a green build here.
    cargo build --release -p hms-server || true

COPY . .
# `touch` so the real sources are newer than the dummy ones cached above.
RUN touch crates/hms-core/src/lib.rs crates/hms-server/src/main.rs \
    && cargo build --release -p hms-server \
    && strip target/release/hms-server

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
