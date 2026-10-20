# Contributing to hilvan

## Development

Requires Rust (see `rust-version` in `Cargo.toml`) with the WebAssembly target, and Node LTS with pnpm:

```sh
rustup target add wasm32-unknown-unknown
```

The app checks typed turns with the server's own code, built to WebAssembly from `crates/check`: `pnpm test` and `pnpm build` in `web/` build it with cargo (`pnpm check-wasm` does only that), and the app imports it from `web/src/check.wasm`, which is built, never committed.

```sh
cp .env.example .env
cargo run -- load-pack packs/en-from-ru/pack.toml   # the formulas the tutor teaches from
cargo run -- serve                        # the API on :8086; /api/health reports the database

cd web && pnpm install && pnpm check-wasm && pnpm dev   # the app on :5173, proxying /api to the server
cd docs/site && pnpm install && pnpm dev  # the documentation site
```

## The demo learner

`demo/learner.json` is three weeks of a fictional learner, built into the binary for `hilvan load-demo` and the screenshots in the docs. It is not edited by hand: a test drives the tutor's own code through those weeks and writes it.

```sh
cargo test --test demo -- --ignored    # writes demo/learner.json
```

Run it when the pack renames what the demo learnt or the learner model changes shape; the gate fails until it is run. The same output comes out every time - the generator has a fixed seed of its own.

## The learner model format

The schema of the learner model file is `docs/site/public/schema/learner-model-2.json`, published with the docs; the schemas of earlier formats stay beside it. A change to the model that a reader of the current format would misread is a new format and a new schema file beside it; the tests hold every export to the schema.
