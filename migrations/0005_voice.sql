-- The voice: sentences that can be said, the sound of them, and who says them.

-- What a substitution turn actually says. The pattern is the scaffold shown
-- on the card - "<pronoun> + am/is/are + <rest>" - and was never a sentence:
-- filled in, it read "I + am/is/are + at home", which cannot be spoken and
-- does not check the one choice the formula teaches. `say` is the sentence
-- itself - "<pronoun> <pronoun:be> <rest>." - and `forms` on each value is
-- what agreement picks from: the pronoun "he" carries be = "is".
--
-- Nullable because a formula without slots has nothing to assemble; the pack
-- validator requires it wherever there is a slot.
ALTER TABLE formula ADD COLUMN say TEXT;
ALTER TABLE slot_value ADD COLUMN forms TEXT NOT NULL DEFAULT '{}';   -- JSON object, form -> text

-- Every piece of speech the tutor has had made, kept so it is made once.
--
-- A Pimsleur-style lesson says the same phrase dozens of times, and a phrase
-- from a paid engine costs credits every time it is made. The key is a hash of
-- everything that changes the sound - engine, voice, model, settings, text -
-- so a different voice or a slower pace is a different entry rather than the
-- wrong sound served from the cache.
--
-- In the database rather than in files: the backup of the tutor is a copy of
-- one file (ADR 0001), and sound that took credits to make belongs in it.
CREATE TABLE audio (
    key         TEXT PRIMARY KEY,     -- SHA-256 hex of engine, voice, model, settings, text
    engine      TEXT NOT NULL,        -- 'piper' | 'elevenlabs'
    voice       TEXT NOT NULL,
    model       TEXT NOT NULL,
    settings    TEXT NOT NULL,        -- JSON, as sent to the engine
    language    TEXT NOT NULL,        -- ISO 639-1
    text        TEXT NOT NULL,
    mime        TEXT NOT NULL,
    bytes       BLOB NOT NULL,
    duration_ms INTEGER NOT NULL,     -- how much speech this is
    credits     INTEGER NOT NULL,     -- what making it cost; 0 for a local engine
    created_at  TEXT NOT NULL,        -- RFC 3339, UTC
    used_at     TEXT NOT NULL,        -- last served; what pruning goes by
    uses        INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE INDEX audio_by_use ON audio(used_at);
CREATE INDEX audio_by_creation ON audio(engine, created_at);

-- The voice the learner chose for a language. No row means the default for
-- that language's role: the engine a language being learnt is spoken by, or
-- the one the learner's own language is.
CREATE TABLE voice_choice (
    language TEXT PRIMARY KEY REFERENCES language(code),
    engine   TEXT NOT NULL,
    voice    TEXT NOT NULL
) STRICT;
