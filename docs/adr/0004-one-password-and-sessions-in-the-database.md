# 0004 · One password as a hash, sessions as rows

Date: 2026-09-03. Status: accepted.

## Context

hilvan has one learner and no accounts, so "authentication" is a single question: is the person holding this browser the one the stand belongs to. The first version answered it with the password itself in the environment and a table of live sessions in the process's memory.

Both were wrong in ways that only show up on a stand rather than in a test.

A password in `HILVAN_PASSWORD` is readable by anything that can read the deployment: the `.env` file travels into whatever backs the Pi up, and `docker inspect` prints the environment of a running container. Nothing about the threat model needs the server to be able to recover the password - only to recognise it.

Sessions in memory are worse in practice than in theory. The stand is updated with `docker compose up -d`, which is a new process, which means every update signs the learner's phone out. A tutor that shows a login screen before a five-minute drill is a tutor that gets skipped, and the versions where that happens are exactly the ones carrying new material worth trying.

rhapsod - the same Pi, the same single reader, the same SQLite - had already settled both questions.

## Decision

- The environment holds **`HILVAN_PASSWORD_HASH`**: an Argon2id PHC string, produced by `hilvan hash`, which prompts so the password stays out of shell history.
- **Sessions are rows** in the database: a 256-bit token, `created_at`, and a `seen_at` refreshed on every request. The lifetime is 90 days and is enforced in the query that checks a token, not by a sweep.
- Unset leaves the stand open, and the server says so once at startup. An open stand has to be a choice, never the result of a forgotten variable.
- The cookie is `HttpOnly` and `SameSite=Lax`, and deliberately not `Secure`: the stand is plain HTTP on a home network, and a cookie the browser refuses to store is a login screen that never goes away.

## Consequences

- **An update does not sign the learner out.** This is the whole reason the rows are in the database, and it is what makes a weekly release cadence compatible with daily study.
- **A leaked `.env` is not a leaked password.** It is an Argon2id hash with a random per-password salt, which is what that format is for.
- **Signing out means something.** A token that cannot be revoked is a promise, not a session; a row can be deleted, and the next request with that cookie is refused.
- **A misconfigured hash is reported as a deployment error, not a wrong password** - otherwise the learner hunts for a password that was never going to work.
- The cost is a migration, one more dependency (`argon2`), and one more step when locking a stand: run `hilvan hash` first. That step is the same one rhapsod already asks for, which is worth more than the minute it costs.
