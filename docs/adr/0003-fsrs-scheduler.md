# 0003 · Spaced repetition: FSRS through `rs-fsrs`

Date: 2026-09-03. Status: accepted.

## Context

Something has to decide what the learner sees today. The choice is between a fixed ladder of intervals (SM-2, the Anki default), a hand-rolled heuristic, and FSRS - the memory model that fits stability and difficulty per card and schedules by predicted recall.

Two crates implement FSRS in Rust. `fsrs` (6.x) is the reference implementation and includes the optimiser, which trains parameters from review history; it pulls in `burn`, a full tensor framework. `rs-fsrs` (1.2) is the scheduler alone: the algorithm, `chrono`, nothing else.

## Decision

**FSRS as the memory model, `rs-fsrs` as the implementation, default parameters to begin with.**

- Reviews are stored in full - rating, timestamp, elapsed days, the resulting stability and difficulty - not just the next due date.
- The scheduler is used through a thin module of hilvan's own, so the crate is an implementation detail rather than a type that spreads through the code.
- Parameter optimisation is deliberately out of scope for now: it needs hundreds of reviews before it says anything, and hilvan has none.

## Consequences

- **The Pi stays a Pi.** `rs-fsrs` adds one small dependency; `fsrs` would add a tensor framework to a container that serves one person.
- **The history is the asset, not the schedule.** Because every review is kept, moving to the optimiser later - or to another model entirely - is a recomputation, not a loss. This is the reason the review table exists separately from the card.
- **Default parameters are wrong for this learner, and that is accepted.** They are the average of many learners; they will schedule too early or too late at first. When there is enough history, an optimiser can be run offline and the parameters written into the settings.
- **The product's three states are hilvan's, not FSRS's.** FSRS has new / learning / review / relearning; the learner sees new, basted and sewn - the stitch the product is named after. The mapping lives in one place, so the vocabulary can change without touching the algorithm.
