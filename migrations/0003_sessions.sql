-- Sessions are rows, not signed tokens: a logout has to be able to end one,
-- and a token that cannot be revoked is not a session but a promise.
--
-- In the database rather than in the process, so that updating the stand -
-- which is a `docker compose up -d`, and will happen every week - does not
-- log the learner's phone out. A tutor that asks for a password before a
-- five-minute drill is a tutor that gets skipped.
CREATE TABLE session (
    token      TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- Refreshed on use, so a learner who misses a few days is not logged out
    -- for having been away.
    seen_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
