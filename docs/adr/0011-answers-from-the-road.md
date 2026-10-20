# 0011 · Answers from the road: one core on both sides, one line on the device

Date: 2026-10-20. Status: accepted.

## Context

The time a learner actually has is on the road: a train, the metro, a queue. Until 0.8 every turn of the drill was checked by `POST /api/formulas/{id}/check` and every answer posted straight to the server, so a sitting needed the tutor in reach the whole way through - and the tutor lives on a home network. The drill could not run underground at all, and an answer whose request failed was simply gone.

Making the drill work there raises three questions: who says whether a typed turn is right when the server cannot be asked, how an answer given hours before it arrives fits into a schedule that has moved on meanwhile, and what happens to an answer delivered twice.

## Decision

**The check is one piece of Rust, run on both sides.** The answer checker, the `say` templates and the grade a round earns (`answer.rs`, `say.rs`, `judge`, `rate`) move into a crate of their own, `hilvan-core`, with no database, clock or network in it. The server links it; `crates/check` builds it to WebAssembly for the app, with a four-export interface (JSON in through one buffer, JSON out through another) and no bindings generator. The app checks every turn with it, in reach or not; the server checks every turn again when the answer arrives, and that reading is the one the grade rests on. `POST /api/formulas/{id}/check` is removed: one way to check, not two. A port to TypeScript was the alternative and was turned down - a second implementation of what is right would drift from the first the first time either changed, and the learner would be told "right" on the phone and graded wrong at home.

**Every answer takes one road.** The app writes each answer to a line in IndexedDB and delivers the line in order - at once at home, whenever the tutor is next reached otherwise. There is no second path for answers given in reach. A delivery the server refuses for the answer's own sake (404, 400, 422) is dropped and counted; one refused for want of a session waits for a sign-in; anything else waits for later. The same rules as rhapsod's queue (rhapsod ADR 0003).

**An answer carries its identity and its moment.** The device mints `answer_id`; the server keeps a receipt per id - the reply it gave - and a repeated delivery is told the same and recorded once. `answered_at` is the device's clock: the answer is recorded at that moment, and a moment still to come is taken as the moment of arrival.

**A card is always its history played again.** An answer is added to the card's history at its own moment and the whole history is walked through FSRS again, rewriting each review row's step and the card row. This replaces the incremental update rather than sitting beside it as a special case for late answers: an answer that arrives after a later one - two devices on one card - takes its place between them, and the card is what both answers in that order make it. Inventory undo, which already rebuilt the card from its history, now rewrites the review rows too; before, it left the steps of later answers stale.

**A late answer obeys the rules of its own moment.** An answer given during a break ends the break at that moment, however long after it arrives - the break counted on a laptop from the last answer it could see is cut back to the answer the phone gave underground.

**The device holds the day.** A service worker keeps the app, the study reads (Today, formulas, words) and the sounds; Today carries `as_of`, the usual pace of each kind of card and the moments the cards out of the queue come back over the next week. The app lays the answers still on the device over the Today it holds. The questions of a sitting are drawn when Today is read in reach, and their sounds fetched then, so the road has something to say aloud. After an answer the tutor could not take in time, the card says it is kept on the device and does not guess when it comes back: the schedule is the server's alone.

## Consequences

- The drill runs without the tutor in reach: checked on the device, answered into the line, delivered in order later.
- The web build needs the `wasm32-unknown-unknown` target and cargo; the image builds the checker in a stage of its own, and CI's web job installs the target.
- Every answer replays its card's history. A card has tens of reviews, so the cost is nothing, and the rows can never drift from what the history says.
- The receipts grow by one row per answer. They are delivery bookkeeping, not part of the learner model: the model file does not carry them, and an import leaves them as they are.
- The API breaks: answers without `answer_id` and `answered_at` are refused, and the check endpoint is gone. There are no other clients.
