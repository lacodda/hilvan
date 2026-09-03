# The stand is a Raspberry Pi, so this image is built for aarch64. Building it
# on the Pi itself keeps the architecture honest: no cross-compilation, and no
# chance of shipping an x86 binary that only fails on the stand. The publish
# workflow builds the same file on native arm64 and amd64 runners.

# ---------------------------------------------------------------- the SPA
FROM node:22-slim AS web
WORKDIR /web

# Dependencies first, so a source-only change does not reinstall them.
COPY web/package.json web/pnpm-lock.yaml ./
RUN corepack enable && corepack prepare --activate && pnpm install --frozen-lockfile

COPY web/ ./
RUN pnpm build

# ------------------------------------------------------------- the server
FROM rust:1-slim-trixie AS server
WORKDIR /src

# A stub crate lets the dependency build cache survive source changes, which
# is what makes a rebuild on a Pi bearable rather than a coffee break.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs && cargo build --release && rm -rf src

COPY src/ src/
COPY migrations/ migrations/
# Migrations are embedded at compile time, so a new one has to invalidate the
# build. Touching the entry points is what tells cargo the stub is stale.
RUN touch src/main.rs src/lib.rs && cargo build --release

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
