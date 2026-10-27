-- The reader: texts the learner brings, words they take from them, and what
-- every word of the language means.

-- A text the learner reads: pasted, or imported from a file. Learner data,
-- like their own words - a reload of a pack leaves it alone, and the learner
-- model file carries it. Its sentences live in `sentence` beside the pack's,
-- so a sentence a text shares with a formula is one sentence.
CREATE TABLE text (
    id          INTEGER PRIMARY KEY,
    language    TEXT NOT NULL REFERENCES language(code),
    title       TEXT NOT NULL,
    format      TEXT NOT NULL CHECK (format IN ('plain', 'markdown', 'subtitles')),
    added_at    TEXT NOT NULL,   -- RFC 3339, UTC
    opened_at   TEXT,            -- last opened; NULL for a text never opened
    paragraph   INTEGER NOT NULL DEFAULT 0,   -- where the learner stopped, from 0
    finished_at TEXT             -- when the learner said they had read it
) STRICT;

-- The sentences of a text, in order, each in the paragraph it stands in.
CREATE TABLE text_sentence (
    text_id     INTEGER NOT NULL REFERENCES text(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,   -- through the whole text, from 0
    paragraph   INTEGER NOT NULL,   -- from 0
    sentence_id INTEGER NOT NULL REFERENCES sentence(id),
    PRIMARY KEY (text_id, position)
) STRICT;

CREATE INDEX text_sentence_by_sentence ON text_sentence(sentence_id);

-- A word the learner put into the deck from a sentence they read: the
-- sentence becomes one the word has been met in, the word is asked in it,
-- and it outlives the text it came from. Keyed like a word's card, on the
-- language's lemma: most words of a text are in no pack.
CREATE TABLE pick (
    word_id     TEXT NOT NULL,   -- `en:suddenly`
    sentence_id INTEGER NOT NULL REFERENCES sentence(id),
    form        TEXT NOT NULL,   -- as it is spelt there
    start       INTEGER NOT NULL,   -- the character it starts at
    picked_at   TEXT NOT NULL,   -- RFC 3339, UTC
    PRIMARY KEY (word_id, sentence_id)
) STRICT;

CREATE INDEX pick_by_sentence ON pick(sentence_id);

-- What the lemmas of a language mean in a pack's native language: the
-- pack's glossary, loaded and replaced with the pack. A word the pack
-- teaches keeps its own gloss (`word.gloss`, the sense its sentences use);
-- this is what every other word means.
CREATE TABLE gloss (
    pack_id TEXT NOT NULL REFERENCES pack(id) ON DELETE CASCADE,
    word_id TEXT NOT NULL,   -- `en:house`
    text    TEXT NOT NULL,
    PRIMARY KEY (pack_id, word_id)
) STRICT;

CREATE INDEX gloss_by_word ON gloss(word_id);

-- Where a word has been met, and since when: a sentence of a formula the
-- learner has started, which the pack marks the word in, or a sentence the
-- learner took the word from. A word is asked only in a sentence it has
-- been met in, and counts as owed only from the moment it first was - the
-- one definition the queue, the debt and the word card all read.
CREATE VIEW met (word_id, sentence_id, met_at) AS
SELECT sw.word_id, sw.sentence_id, fc.introduced_at
FROM sentence_word sw
JOIN formula_sentence fs ON fs.sentence_id = sw.sentence_id
JOIN card fc ON fc.kind = 'formula' AND fc.subject_id = fs.formula_id AND fc.state != 'new'
UNION ALL
SELECT word_id, sentence_id, picked_at FROM pick;
