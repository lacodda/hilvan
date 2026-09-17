# Contributing to hilvan

## Development

Requires Rust (see `rust-version` in `Cargo.toml`) and Node LTS with pnpm.

```sh
cp .env.example .env
cargo run -- load-pack packs/en-from-ru/pack.toml   # the formulas the tutor teaches from
cargo run -- serve                        # the API on :8086; /api/health reports the database

cd web && pnpm install && pnpm dev        # the app on :5173, proxying /api to the server
cd docs/site && pnpm install && pnpm dev  # the documentation site
```
