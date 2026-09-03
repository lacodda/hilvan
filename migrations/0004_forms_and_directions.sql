-- Three forms of one formula, and two directions of one card.
--
-- v0.2.0 adds two things the drill needs and the model did not have a place
-- for: which formulas are the same shape said differently (a statement, its
-- negation, its question), and which way round a card is being asked.

-- The forms of a shape. A formula belongs to a family - `be-present` - and
-- fills one slot in it: statement, negation or question. Kept as two columns
-- on `formula` rather than a `family` table, because a family has no property
-- of its own that the pack does not already say; it is a grouping, not a
-- thing.
--
-- Both are nullable: a formula that is nobody's statement and nobody's
-- question - `let-s-verb`, `how-much-many` - has no family, and the drill
-- shows it without a switch.
ALTER TABLE formula ADD COLUMN family TEXT;
ALTER TABLE formula ADD COLUMN form TEXT;   -- 'statement' | 'negation' | 'question'

CREATE INDEX formula_by_family ON formula(family, form);

-- Direction lives in `card.kind`, which is why that column was a string from
-- the start rather than a foreign key: the pair (kind, subject_id) says what
-- is being scheduled, and 'formula' vs 'formula-recognise' are two separate
-- schedules over the same formula.
--
-- Producing a sentence and understanding one are different skills with
-- different curves - recognition always runs ahead - so they are two cards,
-- and the gap between them is a thing the learner can see rather than a thing
-- averaged away.
--
-- Every formula that already has a producing card gets a recognising one,
-- new and due now. The producing cards keep their history untouched: a week
-- of drilling is not spent on a migration.
INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
SELECT 'formula-recognise', subject_id, 'new', 0.0, 0.0,
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 0, 0
FROM card
WHERE kind = 'formula';
