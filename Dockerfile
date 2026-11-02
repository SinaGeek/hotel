# HMS server image (README §17). Build from the repository root:
#   docker build -f Dockerfile -t hms-server .
#
# The server speaks plain HTTP on :8080 and must sit behind a TLS-terminating
# reverse proxy (deploy/Caddyfile or deploy/nginx.conf). Migrations are
# embedded at compile time by `sqlx::migrate!("../../migrations")`, so the
# whole workspace is copied in.

# ---------- build ----------
FROM rust:1.83-bookworm AS build
WORKDIR /src

# Dependency layer: copy every workspace member's manifest plus a stub source
# for each, so cargo can resolve and compile third-party crates once and reuse
# that layer while only application code changes.
COPY Cargo.toml Cargo.lock ./
COPY crates/hms-core/Cargo.toml crates/hms-core/Cargo.toml
COPY crates/hms-server/Cargo.toml crates/hms-server/Cargo.toml
COPY crates/hms-sync/Cargo.toml crates/hms-sync/Cargo.toml
COPY crates/hms-tests/Cargo.toml crates/hms-tests/Cargo.toml
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
# Stub every member. The desktop crate's build script needs the Tauri webview
# assets, which do not belong in a server image, so its source stays a plain
# `fn main` and it is excluded from the warm-up build below.
RUN set -eux; \
    for d in crates/hms-core crates/hms-server crates/hms-sync crates/hms-tests \
             apps/desktop/src-tauri; do mkdir -p "$d/src"; done; \
    for lib in crates/hms-core crates/hms-sync crates/hms-tests; do \
        : > "$lib/src/lib.rs"; done; \
    echo 'fn main() {}' > crates/hms-server/src/main.rs; \
    echo 'fn main() {}' > apps/desktop/src-tauri/src/main.rs; \
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
