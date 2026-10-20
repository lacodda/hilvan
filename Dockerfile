# The stand is a Raspberry Pi, so this image is built for aarch64. Building it
# on the Pi itself keeps the architecture honest: no cross-compilation, and no
# chance of shipping an x86 binary that only fails on the stand. The publish
# workflow builds the same file on native arm64 and amd64 runners.

# ------------------------------------------------------------ the checker
# The core built to WebAssembly: the app checks a typed turn with the code
# the server grades it with (crates/check). Built here, where cargo is, and
# handed to the Node stage below, where it is not.
FROM rust:1-slim-trixie AS checker
WORKDIR /src
RUN rustup target add wasm32-unknown-unknown
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# The server's own sources are not needed for the checker, but the workspace
# names the server as its root package, and cargo wants it to have a target.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && cargo build -p hilvan-check --target wasm32-unknown-unknown --profile wasm

# ---------------------------------------------------------------- the SPA
FROM node:22-slim AS web
WORKDIR /web

# Dependencies first, so a source-only change does not reinstall them.
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN corepack enable && corepack prepare --activate && pnpm install --frozen-lockfile

COPY web/ ./
COPY --from=checker /src/target/wasm32-unknown-unknown/wasm/hilvan_check.wasm /checker/hilvan_check.wasm
RUN HILVAN_CHECK_WASM=/checker/hilvan_check.wasm pnpm build

# ------------------------------------------------------------- the server
FROM rust:1-slim-trixie AS server
WORKDIR /src

# A stub crate lets the dependency build cache survive source changes, which
# is what makes a rebuild on a Pi bearable rather than a coffee break. The
# core is a crate of the workspace the server depends on, so it comes in
# with the manifests.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs && cargo build --release -p hilvan && rm -rf src

COPY src/ src/
COPY migrations/ migrations/
COPY lexicons/ lexicons/
COPY demo/ demo/
# Migrations, the lexicon and the demo learner are embedded at compile time,
# so a change to any of them has to invalidate the build. Touching the entry
# points is what tells cargo the stub is stale.
RUN touch src/main.rs src/lib.rs && cargo build --release -p hilvan

# ------------------------------------------------------------- the runtime
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Unprivileged: nothing here needs root, and a web-facing process least of all.
RUN useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin hilvan
WORKDIR /app

COPY --from=server /src/target/release/hilvan /usr/local/bin/hilvan
COPY --from=web /web/dist /app/web
# The formula packs travel with the image: `hilvan load-pack` is run inside
# the container, and material the tutor teaches from is part of the product,
# not something to copy onto the stand by hand.
COPY packs/ /app/packs

# The one volume: the database, which is the only thing written. It has to
# exist in the image and be owned by the runtime user - Docker copies
# ownership onto a fresh named volume from what the image has at the mount
# point, and an absent path leaves the volume root-owned.
RUN mkdir -p /data && chown -R 10001:10001 /data

ENV HILVAN_DATABASE_URL=sqlite:///data/hilvan.db?mode=rwc \
    HILVAN_WEB_DIR=/app/web \
    HILVAN_ADDR=0.0.0.0:8086
EXPOSE 8086
VOLUME ["/data"]
USER 10001

# /api/health round-trips to the database, so a container that answers has
# actually reached its storage.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8086/api/health || exit 1

CMD ["hilvan", "serve"]
