-- Answers arrive from the device's queue: at once at home, an evening later
-- after a day underground, and now and then twice - the reply to the first
-- delivery lost on the way back, and the device sending it again.
--
-- Each answer carries an identity its device minted and the moment it was
-- given by the device's clock. The review is recorded at that moment, and
-- the card is its history played through the scheduler again (src/study.rs,
-- `replay`), so an answer that lands after a later one takes its place in
-- the history rather than the end of it.
--
-- This is the receipt: one row per answer taken, with what the device was
-- told, so a repeated delivery is told the same and recorded once. It is
-- bookkeeping of delivery, not of the learner: the learner model file does
-- not carry it, and an import leaves it as it is.
CREATE TABLE answer (
    id          TEXT PRIMARY KEY,   -- minted by the device that took the answer
    answered_at TEXT NOT NULL,      -- RFC 3339, UTC: the device's clock, no later than arrival
    arrived_at  TEXT NOT NULL,      -- RFC 3339, UTC: the server's clock
    reply       TEXT NOT NULL       -- JSON: what the device was told, told again on a repeat
) STRICT;
