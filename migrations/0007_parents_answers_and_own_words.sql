-- Formulas stand on parents, answers are typed and checked, and the learner
-- brings words of their own.

-- The shapes a formula is built from: "I would like" stands on "I want". A
-- formula opens only once every parent is sewn. Pack data, replaced with the
-- formulas on a reload; the pack validator keeps every parent earlier in the
-- pack's sequence than its child, so the tree cannot hold a cycle.
CREATE TABLE formula_parent (
    formula_id TEXT NOT NULL REFERENCES formula(id) ON DELETE CASCADE,
    parent_id  TEXT NOT NULL REFERENCES formula(id) ON DELETE CASCADE,
    PRIMARY KEY (formula_id, parent_id)
) STRICT;

CREATE INDEX formula_parent_by_parent ON formula_parent(parent_id);

-- Other ways of saying what `say` says that are just as right: "I'd like"
-- for "I would like". Templates in the syntax of `say`, as a JSON array; the
-- forms they ask for live on the values, like the forms `say` asks for.
ALTER TABLE formula ADD COLUMN accept TEXT NOT NULL DEFAULT '[]';

-- The same for a fixed sentence: "I'm not hungry." accepts "I am not hungry."
-- On the sentence rather than on the formula's link to it, because a
-- sentence two formulas show is one sentence with one set of spellings.
ALTER TABLE sentence ADD COLUMN accept TEXT NOT NULL DEFAULT '[]';

-- Which of the learner's own words a slot takes: 'noun' or 'verb', or NULL
-- for a slot that takes only the pack's values. A slot that takes them asks
-- no forms of its value - the validator sees to it - so a word the learner
-- typed in fits without the code knowing any grammar.
ALTER TABLE slot ADD COLUMN takes TEXT;

-- Words from the learner's own life, substituted into the slots that take
-- them. Learner data rather than pack data: a reload of the pack leaves them
-- alone, and they belong to the language being learnt, not to a pack.
CREATE TABLE own_word (
    id         INTEGER PRIMARY KEY,
    language   TEXT NOT NULL REFERENCES language(code),
    kind       TEXT NOT NULL CHECK (kind IN ('noun', 'verb')),
    native     TEXT NOT NULL,   -- in the learner's own language
    target     TEXT NOT NULL,   -- as it is said in a sentence: "a car", "swim"
    created_at TEXT NOT NULL,   -- RFC 3339, UTC
    UNIQUE (language, kind, target)
) STRICT;

-- Every typed turn of a graded formula, kept forever like the review it
-- belongs to: the real mistakes, which later versions pair into confusions
-- and explain.
CREATE TABLE attempt (
    id        INTEGER PRIMARY KEY,
    review_id INTEGER NOT NULL REFERENCES review(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,     -- the turn, from 0
    prompt    TEXT NOT NULL,        -- what the learner read, in their own language
    expected  TEXT NOT NULL,        -- the answer it was checked against
    typed     TEXT NOT NULL,        -- what the learner typed, as typed
    correct   INTEGER NOT NULL,     -- 0 or 1
    failure   TEXT,                 -- 'formula' | 'order' | 'word'; NULL when correct
    missed    TEXT NOT NULL DEFAULT '[]',   -- JSON array of the expected words that did not come
    UNIQUE (review_id, position)
) STRICT;
