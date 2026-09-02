# 0001 · Server stack: axum, SQLite, a React SPA

Date: 2026-09-02. Status: accepted.

## Context

hilvan is a tutor for one learner, running on a Raspberry Pi 4 at home alongside other services of the line. The lacodda line already has a production-proven server shape (kasl-server, lyrid): Rust with axum and sqlx, a React SPA built with Vite, CI with an MSRV job, tag-driven releases, a Starlight docs site. Those two products serve many users and run PostgreSQL in a second container.

hilvan has one user. Its state is small: the learner model (known formulas and words), review schedules, generated material, an audio cache. What matters on the Pi is that the thing keeps running with nobody watching it, that a backup is trivial, and that a restore after a dead SD card is a file copied back.

## Decision

- Backend: Rust, **axum** + **sqlx** over **SQLite** - one file, WAL mode, foreign keys on, migrations embedded in the binary and applied at start.
- Frontend: **React SPA** (Vite, TypeScript, Tailwind) on the line's design system, **dowel**. The SPA is served by the same process; the server is the only thing listening on the stand.
- Production rails follow the line's template: fmt/clippy/test/msrv CI on Linux and Windows, releases by tag with git-cliff notes, a container image published to GHCR for amd64 and arm64, a Starlight docs site with the Diátaxis structure.
- Edition 2024; the MSRV is declared in the manifest and held by a CI job that builds on it.

## Consequences

- **One container, one volume for state.** There is no database service to keep healthy, no credentials to rotate, no second image to pull on a Pi. `docker compose up` is the whole installation.
- **Backups are a copy.** The `data` volume holds one file; copying it while the server runs is safe under WAL, and restoring it is copying it back.
- **Single user by construction.** SQLite's writer lock is a non-issue for one person on a phone. If hilvan ever needs to serve several people, that is a new product decision, not a migration to plan for now.
- **sqlx queries are checked at runtime, not compile time.** The line's offline query checking against PostgreSQL does not carry over as is; tests run against a real file, migrated from empty, so the schema is exercised on every push.
- **Infrastructure is copied, not invented.** Everything outside the SQLite choice is the line's known-good shape, which is what lets the first version be a skeleton that already builds, lints and ships.
