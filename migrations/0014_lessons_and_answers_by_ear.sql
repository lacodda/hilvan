-- Audio lessons, and the answers heard in them.
--
-- A lesson is today's queue said aloud, Pimsleur's way: a question, a pause
-- to answer in, the answer, room to say it again. It is planned from what
-- is due, spoken line by line through the audio cache, and spliced into one
-- file with chapters by phrase, which the app takes on the road. The file
-- lives here, like every other sound the tutor makes: one volume to back up.
--
-- An answer heard in a lesson is graded by the learner afterwards - held or
-- slipped - and moves the card like any other. It is marked, because an
-- answer said aloud on a walk and one typed at a desk are not the same
-- evidence, and a later reading of the history should be able to tell.

CREATE TABLE lesson (
    id         INTEGER PRIMARY KEY,
    asked_at   TEXT NOT NULL,              -- RFC 3339, UTC: when it was asked for
    minutes    INTEGER NOT NULL CHECK (minutes IN (15, 25, 40)),
    state      TEXT NOT NULL CHECK (state IN ('building', 'ready', 'failed')),
    error      TEXT,                       -- why a failed lesson failed, as the learner reads it
    done       INTEGER NOT NULL DEFAULT 0, -- lines spoken so far, while building
    total      INTEGER NOT NULL DEFAULT 0, -- lines it needs
    built_at   TEXT,                       -- RFC 3339, UTC: when it was ready
    length_ms  INTEGER,                    -- how long it lasts
    plan       TEXT,                       -- JSON: its phrases, with the cards they answer, and its chapters
    credits    INTEGER NOT NULL DEFAULT 0, -- what its new lines cost the paid voice
    bytes      BLOB                        -- the file, audio/mpeg
) STRICT;

CREATE INDEX lesson_by_state ON lesson(state, id);

ALTER TABLE review ADD COLUMN heard INTEGER NOT NULL DEFAULT 0 CHECK (heard IN (0, 1));
