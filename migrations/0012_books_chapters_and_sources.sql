-- Books: a text has chapters, an author and the link it came from, and the
-- formats a book or a page is written in.
--
-- `text` is built again for its id to count up and never be given to
-- another text: the server keeps the reading of a text by its id while it
-- runs, and the app keeps a text read once by its address, so a number
-- handed on from a text let go would show one text as another.
--
-- Dropping `text` with its sentences still pointing at it would delete
-- them, so they are set aside first and put back after.

CREATE TABLE text_sentence_kept AS SELECT text_id, position, paragraph, sentence_id FROM text_sentence;
DROP TABLE text_sentence;

CREATE TABLE text_counted (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    language    TEXT NOT NULL REFERENCES language(code),
    title       TEXT NOT NULL,
    author      TEXT,            -- as the book or the page names it
    source      TEXT,            -- the link it came from; NULL for a paste or a file
    format      TEXT NOT NULL CHECK (format IN ('plain', 'markdown', 'subtitles', 'html', 'wiki', 'epub', 'fb2')),
    added_at    TEXT NOT NULL,   -- RFC 3339, UTC
    opened_at   TEXT,            -- last opened; NULL for a text never opened
    paragraph   INTEGER NOT NULL DEFAULT 0,   -- where the learner stopped, through the whole text, from 0
    finished_at TEXT             -- when the learner said they had read it
) STRICT;

INSERT INTO text_counted (id, language, title, format, added_at, opened_at, paragraph, finished_at)
SELECT id, language, title, format, added_at, opened_at, paragraph, finished_at FROM text;
DROP TABLE text;
ALTER TABLE text_counted RENAME TO text;

-- The sentences of a text, in order, each in the paragraph it stands in.
CREATE TABLE text_sentence (
    text_id     INTEGER NOT NULL REFERENCES text(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,   -- through the whole text, from 0
    paragraph   INTEGER NOT NULL,   -- through the whole text, from 0
    sentence_id INTEGER NOT NULL REFERENCES sentence(id),
    PRIMARY KEY (text_id, position)
) STRICT;

INSERT INTO text_sentence SELECT text_id, position, paragraph, sentence_id FROM text_sentence_kept;
DROP TABLE text_sentence_kept;
CREATE INDEX text_sentence_by_sentence ON text_sentence(sentence_id);

-- The chapters of a text: each starts at a paragraph and runs to the next
-- one's. Every text has one at least, starting at its first paragraph; a
-- book has the chapters its source gives.
CREATE TABLE chapter (
    text_id   INTEGER NOT NULL REFERENCES text(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL CHECK (position >= 0),   -- from 0
    title     TEXT,                                     -- NULL where the source names none
    paragraph INTEGER NOT NULL CHECK (paragraph >= 0 AND (position > 0 OR paragraph = 0)),
    PRIMARY KEY (text_id, position),
    UNIQUE (text_id, paragraph)
) STRICT;

INSERT INTO chapter (text_id, position, title, paragraph) SELECT id, 0, NULL, 0 FROM text;
