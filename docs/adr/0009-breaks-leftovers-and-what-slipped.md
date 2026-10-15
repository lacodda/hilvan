# 0009 · Breaks stop the clock, leftovers take new places, and what slipped is named

Date: 2026-10-15. Status: accepted.

## Context

v0.7.0 is about the schedule surviving a life rather than only discipline. FSRS, like every spaced-repetition scheduler, assumes a learner who comes every day. The learner this tutor has does not, and the failure that follows is well known: a few days away, a wall of reviews on the way back, and the tutor abandoned.

**A holiday turns into a debt.** Two weeks away leave two weeks of reviews due on the first evening home - more than a sitting holds, on the evening least likely to have one.

**New material does not know about the debt.** The day's one new formula and ten new words open whatever the reviews look like, so a learner catching up gets a full day's new load on top, and the reviews it brings arrive in a week, when the catching up has not finished.

**Forgetting is invisible.** A card answered *again* goes back into the queue and is never mentioned: the learner cannot tell a week where three sewn formulas came loose from a week where nothing did.

**Stability is a number nobody reads.** The card knows how long it holds - FSRS's stability, in days - and shows nothing of it, or at best a figure like 17.3 that says nothing at a glance.

## Decision

**A break stops the schedule's clock.** A break is a row: a start, and an end or none (until the learner says they are back). An interval runs only in time outside breaks, so a card whose interval spans a break comes back later by the part of the break it spans; a card due before a break started is not touched by it. The card row keeps the due FSRS gave it - the row stays the last step of its answers played through the scheduler, which the snapshots of ADR 0008 rest on - and the breaks are applied when the queue is read, by one pure function (`scheduling::comes_back`). FSRS is told the truth: the next answer counts every day since the last one, holiday included.

Every interval that spans a break moves, not only the ones that fell due during it. Moving only those - spreading them over as many days after the break - would put the break's reviews on top of the reviews of the days after it: twice the load for as long as the learner was away, the avalanche spread thinner rather than gone.

**An absence can be counted as a break afterwards.** When the last answer was three days ago or more, no break reaches past it and something is waiting, Today offers to count the time since as a break: one tap writes a break from the last answer to now. Two days - a weekend - are not offered; what came due over them is a sitting's worth and should be owed. It is the same row as a break taken ahead, and the schedule cannot tell them apart. A break holds no asked answer: one cannot be counted back past the last answer, and an answer given during a break ends it.

**Leftovers take new places.** A card the queue asks that came due - the breaks counted - before the start of yesterday, and so waited through a whole day it could have been answered on, is owed. Each owed card takes one of the day's ten new words; when they have taken all ten, the day's new formula waits too, since a new shape brings its sentences' words and there would be no room for them. The debt is counted as the day began - a card answered since midnight is set back to how it stood then by playing its earlier answers through the scheduler - so paying it does not open another round the same day. A day of grace keeps a learner who studies every evening out of debt for the clock drifting past the sitting. A word is owed from when a sentence holding it was first met, not from a due date it had before (a word declared known has one from the inventory).

**A slip is an again after a hold.** A slip is an asked *again* on a card whose answer before it was not *again* and was given at least a day earlier (FSRS's `elapsed_days ≥ 1`). It catches a sewn formula that came loose, a basted word that had come on an earlier day, and a word declared known that is not; it leaves out learning steps within a day and a card that never came at all. It is read off the review log with a window function - no stored flag. Slips are listed by week, Monday to Sunday UTC; last week's open Today while the current week has no asked answer yet, which is also what closes the note. Nothing records that the note was shown.

**Stability is a bar.** The card draws it on a logarithmic scale from an hour to two years, cut into hours, days, weeks, months and a year and more, and says the band in words. After an answer the bar shows the stretch it grew by, or the stretch it lost.

**The learner model file is format 2.** It carries the breaks; format 1 is refused. Schema `learner-model-2.json` is published beside `learner-model-1.json`, which stays for readers of older files.

## Consequences

- Every place that asks "is it due" asks it through the breaks: today's formulas, today's words, the debt. A place that read `card.due` alone would bring a card back during a break.
- A snapshot carries the breaks begun by then, one that had not ended as still going on.
- Breaks are left out of the memory model on purpose. Fitting FSRS to this learner later sees long gaps after breaks as long gaps, which they were.
- The room for new material now depends on the past day; a test that drives the tutor through several days (the demo) changes when the rule does, and is made again.
- What slipped is only what was caught: a card never asked during a silent week did not slip on record, however much it faded. The debt line is what speaks for those.
