-- The learner model: what is being learnt, and how well.
--
-- One model for every language (ADR 0002): a language is a column, never a
-- branch in the code, so adding Spanish adds rows rather than tables.

-- Languages the learner has a relationship with. `native` marks the one
-- prompts are written in; the others are being learnt.
CREATE TABLE language (
    code      TEXT PRIMARY KEY,           -- ISO 639-1: en, ru, es
    name      TEXT NOT NULL,              -- English name, for the interface
    is_native INTEGER NOT NULL DEFAULT 0  -- 0 or 1
) STRICT;

-- A pack is a file of material loaded into the database. The row records
-- which version of which file the formulas below came from, so a reload can
-- tell "already have this" from "this changed".
CREATE TABLE pack (
    id       TEXT PRIMARY KEY,   -- matches pack.toml's id, e.g. en-from-ru
    version  INTEGER NOT NULL,
    native   TEXT NOT NULL REFERENCES language(code),
    target   TEXT NOT NULL REFERENCES language(code),
    loaded_at TEXT NOT NULL      -- RFC 3339, UTC
) STRICT;

-- A formula: an assembly pattern the learner builds sentences from. Not a
-- rule to know, a shape to produce.
CREATE TABLE formula (
    id          TEXT PRIMARY KEY,   -- from the pack, stable across versions
    pack_id     TEXT NOT NULL REFERENCES pack(id) ON DELETE CASCADE,
    target      TEXT NOT NULL REFERENCES language(code),
    name        TEXT NOT NULL,
    pattern     TEXT NOT NULL,
    explanation TEXT NOT NULL,      -- in the pack's native language
    position    INTEGER NOT NULL    -- the order formulas are introduced in
) STRICT;

CREATE INDEX formula_by_position ON formula(position, id);

-- A worked example of the formula: what a correct answer looks like.
CREATE TABLE sample (
    id         INTEGER PRIMARY KEY,
    formula_id TEXT NOT NULL REFERENCES formula(id) ON DELETE CASCADE,
    native     TEXT NOT NULL,
    target     TEXT NOT NULL,
    position   INTEGER NOT NULL
) STRICT;

CREATE INDEX sample_by_formula ON sample(formula_id, position);

-- A slot is a hole in the pattern the drill substitutes into; a value is one
-- filling for it. Kept as two tables rather than a JSON blob so a value can
-- later carry its own audio and its own review state.
CREATE TABLE slot (
    id         INTEGER PRIMARY KEY,
    formula_id TEXT NOT NULL REFERENCES formula(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,   -- matches a <placeholder> in the pattern
    position   INTEGER NOT NULL,
    UNIQUE (formula_id, name)
) STRICT;

CREATE TABLE slot_value (
    id       INTEGER PRIMARY KEY,
    slot_id  INTEGER NOT NULL REFERENCES slot(id) ON DELETE CASCADE,
    native   TEXT NOT NULL,
    target   TEXT NOT NULL,
    position INTEGER NOT NULL
) STRICT;

CREATE INDEX slot_value_by_slot ON slot_value(slot_id, position);

-- What the tutor schedules. A card is the reviewable side of something the
-- learner is learning; today only formulas have cards, words get theirs in
-- v0.4.0, which is why the subject is a pair (kind, subject_id) rather than a
-- foreign key to `formula`.
--
-- The FSRS state lives here: stability and difficulty are the memory model's
-- two numbers, `due` is when it next surfaces, `state` is the word the
-- product uses for it - new, basted, sewn (the stitch the name comes from).
CREATE TABLE card (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,       -- 'formula' today
    subject_id   TEXT NOT NULL,
    state        TEXT NOT NULL,       -- 'new' | 'basted' | 'sewn'
    stability    REAL NOT NULL,       -- FSRS: days until recall drops to 90%
    difficulty   REAL NOT NULL,       -- FSRS: 1..10
    due          TEXT NOT NULL,       -- RFC 3339, UTC
    reps         INTEGER NOT NULL DEFAULT 0,
    lapses       INTEGER NOT NULL DEFAULT 0,
    last_reviewed TEXT,               -- RFC 3339, UTC; NULL until first review
    introduced_at TEXT,               -- when it stopped being new
    UNIQUE (kind, subject_id)
) STRICT;

CREATE INDEX card_by_due ON card(due, id);

-- Every answer, kept forever: the schedule can be recomputed from history
-- when the memory model changes, which it will.
CREATE TABLE review (
    id            INTEGER PRIMARY KEY,
    card_id       INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    rating        INTEGER NOT NULL,   -- 1 again, 2 hard, 3 good, 4 easy
    reviewed_at   TEXT NOT NULL,      -- RFC 3339, UTC
    elapsed_days  REAL NOT NULL,      -- since the previous review
    stability     REAL NOT NULL,      -- state after this answer
    difficulty    REAL NOT NULL,
    duration_ms   INTEGER             -- how long the answer took, when known
) STRICT;

CREATE INDEX review_by_card ON review(card_id, reviewed_at);
