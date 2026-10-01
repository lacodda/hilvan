-- Words, and the sentences they live in.
--
-- v0.4.0 makes the sentence a thing of its own. Until now a worked example
-- was a row of its formula - native and target side by side, gone and
-- rewritten on every reload of the pack. A word needs more than that: it is
-- learnt inside a sentence, it is heard in one sentence of its own (the
-- anchor), and that sentence has to stay the same sentence when the pack is
-- reloaded with a corrected translation.

-- A sentence of a language, identified by what it says. Two formulas that
-- show the same example share one row, which is what lets a word met in
-- three places be one word with three contexts rather than three words.
CREATE TABLE sentence (
    id       INTEGER PRIMARY KEY,
    language TEXT NOT NULL REFERENCES language(code),
    text     TEXT NOT NULL,
    UNIQUE (language, text)
) STRICT;

-- What a sentence means in another language. A table rather than a column:
-- the meaning belongs to a pair of languages, and the English sentence is
-- the same sentence for a learner coming from Russian and one coming from
-- Spanish.
CREATE TABLE sentence_translation (
    sentence_id INTEGER NOT NULL REFERENCES sentence(id) ON DELETE CASCADE,
    language    TEXT NOT NULL REFERENCES language(code),
    text        TEXT NOT NULL,
    PRIMARY KEY (sentence_id, language)
) STRICT;

-- The worked examples of a formula, in the order the pack gives them.
CREATE TABLE formula_sentence (
    formula_id  TEXT NOT NULL REFERENCES formula(id) ON DELETE CASCADE,
    sentence_id INTEGER NOT NULL REFERENCES sentence(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    PRIMARY KEY (formula_id, position)
) STRICT;

CREATE INDEX formula_sentence_by_sentence ON formula_sentence(sentence_id);

-- A word a pack teaches: a lemma of the language being learnt, and what it
-- means in the learner's own. How common it is and how it sounds are not
-- here - they belong to the language, and the lexicon built into hilvan
-- says them (src/lexicon.rs). One truth each.
--
-- The id is the language and the lowercase lemma, `en:doctor`: what the
-- word's card keys on, so that a reload rewriting this row - a better gloss -
-- leaves what the learner knows of the word alone.
CREATE TABLE word (
    id       TEXT PRIMARY KEY,
    pack_id  TEXT NOT NULL REFERENCES pack(id) ON DELETE CASCADE,
    language TEXT NOT NULL REFERENCES language(code),
    lemma    TEXT NOT NULL,   -- as written: "I", "Monday", "doctor"
    gloss    TEXT NOT NULL    -- in the pack's native language
) STRICT;

-- Where a word stands in a sentence: the spelling it takes there ("went"
-- for go) and the character it starts at, which is what the app highlights.
CREATE TABLE sentence_word (
    sentence_id INTEGER NOT NULL REFERENCES sentence(id) ON DELETE CASCADE,
    word_id     TEXT NOT NULL REFERENCES word(id) ON DELETE CASCADE,
    form        TEXT NOT NULL,
    start       INTEGER NOT NULL,
    PRIMARY KEY (sentence_id, word_id)
) STRICT;

CREATE INDEX sentence_word_by_word ON sentence_word(word_id);

-- The one sentence a word is heard in: chosen when the word is first
-- answered and kept, so the word comes back with the same melody every time.
--
-- Learner state, so it keys on the word id rather than on the word row,
-- which a reload replaces. When its sentence goes - the pack dropped or
-- reworded it - the anchor goes with it, and the word is heard in the first
-- sentence it is met in until it is answered again.
CREATE TABLE anchor (
    word_id     TEXT PRIMARY KEY,
    sentence_id INTEGER NOT NULL REFERENCES sentence(id) ON DELETE CASCADE
) STRICT;

-- The worked examples already in the database move into sentences, so a
-- stand upgraded before its pack is reloaded still drills with its examples.
INSERT INTO sentence (language, text)
SELECT DISTINCT p.target, s.target
FROM sample s
JOIN formula f ON f.id = s.formula_id
JOIN pack p ON p.id = f.pack_id;

-- `OR IGNORE`: a sentence shown by two formulas with two different
-- translations keeps the first, and the pack validator from this version on
-- refuses the second.
INSERT OR IGNORE INTO sentence_translation (sentence_id, language, text)
SELECT se.id, p.native, s.native
FROM sample s
JOIN formula f ON f.id = s.formula_id
JOIN pack p ON p.id = f.pack_id
JOIN sentence se ON se.language = p.target AND se.text = s.target
ORDER BY f.position, s.position;

INSERT INTO formula_sentence (formula_id, sentence_id, position)
SELECT s.formula_id, se.id, s.position
FROM sample s
JOIN formula f ON f.id = s.formula_id
JOIN pack p ON p.id = f.pack_id
JOIN sentence se ON se.language = p.target AND se.text = s.target;

DROP TABLE sample;
