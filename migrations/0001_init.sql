-- The first table of the tutor's memory.
--
-- A key-value table for the few things the server has to remember about
-- itself before there is a learner model to speak of: a chosen theme, a
-- schema marker, whatever bootstrapping needs. The learner model - known
-- formulas and words, review schedules, generated material - arrives in its
-- own tables in v0.1.0, not as columns guessed at here.
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
