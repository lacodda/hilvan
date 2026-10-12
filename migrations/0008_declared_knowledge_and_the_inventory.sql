-- Knowledge the learner declares, and the inventory that collects it.

-- Where an answer came from. `asked`: the tutor asked and the learner
-- answered - a formula drilled, a word in its sentence. `declared`: the
-- learner said they know it without being asked - the inventory today, a tap
-- in the reader later. A declaration moves the card the way an easy answer
-- would, so the schedule starts somewhere honest, but it is not an answer:
-- it is not one of the day's answers, it does not spend the day's room for
-- new words, and it is left out when the memory model is fitted to this
-- learner.
ALTER TABLE review ADD COLUMN source TEXT NOT NULL DEFAULT 'asked' CHECK (source IN ('asked', 'declared'));

-- The inventory: the commonest words of a language, each asked once - "do
-- you know it?". Every answer is kept, the no's too: they are what the run
-- would otherwise ask again tomorrow. A yes is also a declared review of the
-- word's card; the row is the question, the review is what it did.
--
-- Keyed on the id a word's card keys on, `en:house`, which is the language's
-- lemma rather than a pack's word: most of the commonest thousand are in no
-- pack, and a pack that brings one later finds it already sewn.
CREATE TABLE inventory (
    word_id     TEXT PRIMARY KEY,
    known       INTEGER NOT NULL CHECK (known IN (0, 1)),
    answered_at TEXT NOT NULL   -- RFC 3339, UTC
) STRICT;
