-- Breaks: stretches of time the schedule's clock stood still.
--
-- A card keeps the due FSRS gave it - the row stays the last step of its
-- answers played through the scheduler - and when it comes back is worked
-- out from that and the breaks (src/scheduling.rs): an interval is counted
-- only in time outside a break, so a fortnight away moves everything that
-- spans it by a fortnight instead of leaving it all owed on the day back.
--
-- A break holds no asked answer. One is started now or counted back from
-- the last answer for an absence nobody declared; an answer given during
-- one ends it.
CREATE TABLE break (
    id        INTEGER PRIMARY KEY,
    starts_at TEXT NOT NULL,   -- RFC 3339, UTC
    ends_at   TEXT,            -- RFC 3339, UTC; NULL while it lasts until the learner is back
    CHECK (ends_at IS NULL OR ends_at > starts_at)
) STRICT;

CREATE INDEX break_by_start ON break(starts_at);
