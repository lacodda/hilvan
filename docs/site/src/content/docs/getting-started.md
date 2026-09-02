---
title: Getting started
description: Run hilvan locally - the server over a SQLite file, the tutor app, and this documentation site.
---

hilvan is one process: a Rust server over a SQLite file, serving a JSON API and a React app. At v0.0.0 there is no learner model yet - no formulas, no repetition, no lessons - so "getting started" means running the pieces on your own machine and seeing them answer. This is the honest state of the skeleton: it builds, lints and ships a container, and that is all it does so far.

## What you need

- **Rust** - at least the version in `rust-version` in `Cargo.toml`. `rustup update stable` is enough.
- **Node LTS with pnpm** - `corepack enable` provides pnpm.
- **Docker** - only to run the image the way the stand does; nothing else needs it.

## The server

```sh
cp .env.example .env
cargo run -- serve
```

`serve` is also what running the binary with no arguments does, which is what a container image or a service unit expects.

The server binds `0.0.0.0:8086` (override with `HILVAN_ADDR`), creates `data/hilvan.db` if it is not there, applies any pending migrations, and serves `/api/health`:

```sh
curl http://127.0.0.1:8086/api/health
```

```json
{"status":"ok","version":"0.0.0"}
```

## The app

```sh
cd web
pnpm install
pnpm dev
```

Vite serves the app on `http://localhost:5173` and proxies `/api` to the server, so the two run as one origin. `pnpm build` writes `web/dist`, which is where the server looks for the app when it serves it itself (`HILVAN_WEB_DIR`).

## This site

```sh
cd docs/site
pnpm install
pnpm dev
```

## First login

There is no login yet: the skeleton has no accounts and no password. A single learner is assumed throughout, and whatever gate the product ends up needing arrives with the feature that needs it, not ahead of it.

## Next

- [Running on a Raspberry Pi](/hilvan/guides/running-on-a-pi/) - the stand.
- [Configuration](/hilvan/reference/configuration/) - every variable the server reads.
