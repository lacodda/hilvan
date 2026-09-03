# 0005 · Forms of a shape, directions of a card, and what pace is allowed to do

Date: 2026-09-03. Status: accepted.

## Context

v0.2.0 asks for three things that all touch the learner model, and each has a version that is cheaper and a version that is honest.

**Forms.** A learner who can say *I am tired* but cannot ask *Are you tired?* has half a formula. The pack already held the three as three separate formulas with three separate schedules (`be-present-statement`, `-negation`, `-question`), which is correct about memory - the negation with `don't` really is learnt apart from the statement - but wrong about the card: nothing on screen said the three were one shape.

**Directions.** Understanding a sentence and producing one are different skills, and recognition always runs ahead. A drill that asks only one way measures only one, and a drill that asks both ways behind one schedule averages the two into a number that describes neither.

**Pace.** How long an answer took is the second dimension of knowing something: stability says whether the formula is still there, time says whether it still costs thought. FSRS cannot see the difference - an answer that arrived after eight seconds of assembly and one that simply arrived are both `good`.

## Decision

**Forms are a relation between formulas, not a structure inside one.** A formula carries an optional `family` and `form`; the three forms of a shape stay three formulas with three schedules, and the card draws a switch between them. Rejected: collapsing the three into one formula with three patterns, which would have changed every id in the pack and thrown away the learner's history for the sake of the screen.

**Each direction is its own card.** `card.kind` distinguishes them: `formula` for producing, `formula-recognise` for understanding. The producing card keeps the kind v0.1.0 wrote, so the migration adds rows and rewrites none. The recognising card of a shape opens only once that shape has been produced at least once, and never counts against the day's budget of one new formula.

**Pace is reported and never scheduled on.** The median of the last eight timed answers, shown on the card and after an answer; the scheduler does not read it.

## Consequences

- **A week of drilling survives the upgrade.** Verified against a real database carried forward from v0.1.2: 24 reviews, stabilities, lapses and states all unchanged, 30 recognising cards added alongside.
- **The queue roughly doubles over time**, since every shape eventually has two cards. This is the honest cost of measuring two skills, and the gap between the two columns on Today is the thing worth seeing.
- **A family is data, so it can be wrong**, and a wrong one is silent in the drill. The pack validator rejects a form without a family, two formulas claiming the same form, and a family holding a single formula.
- **Pace stays comparable across versions.** Because it never enters the schedule, changing how it is computed - or dropping it - cannot invalidate a single interval. The decision can be revisited when there is a year of history to fit against; nothing has to be undone first.
- **The median, not the mean.** One interrupted turn - a phone put down mid-drill - would drag a mean for weeks and make the number untrustworthy at exactly the moment it is glanced at.
