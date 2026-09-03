---
title: Getting started
description: Run hilvan locally - the server over a SQLite file, the tutor app, and this documentation site.
---

hilvan is one process: a Rust server over a SQLite file, serving a JSON API and a React app. As of v0.1.0 there is a learner model - formulas, spaced repetition, the daily drill - but no material of its own until you load a pack, and no lessons or audio yet.

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

## Loading a pack

The server has no material until you load one. `packs/en-from-ru/pack.toml` ships with the repository - 30 English formulas for a Russian speaker - and loading it is idempotent, so it is safe to run again:

```sh
cargo run -- load-pack packs/en-from-ru/pack.toml
```

See [Formula packs](/hilvan/reference/packs/) for the file format and what loading a changed pack does.

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

## Signing in

There are no accounts - hilvan serves one learner. Run `hilvan hash`, put the string it prints in `.env` as `HILVAN_PASSWORD_HASH` (single-quoted), and the study endpoints ask for the password once, with a session that lasts 90 days and survives a restart of the server; leave it unset and the stand is open, which is the default and what a developer's machine wants. See [Configuration](/hilvan/reference/configuration/#the-password).

## Next

- [Your first week](/hilvan/guides/your-first-week/) - loading the pack, signing in, and what a sitting looks like.
- [Running on a Raspberry Pi](/hilvan/guides/running-on-a-pi/) - the stand.
- [Configuration](/hilvan/reference/configuration/) - every variable the server reads.
- [HTTP API](/hilvan/reference/api/) - every endpoint under `/api`.
