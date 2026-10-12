# 0008 · Declared knowledge, the learner model as one file, and the past as a replay

Date: 2026-10-12. Status: accepted.

## Context

v0.6.0 is about the learner model leaving the inside of the tutor: a learner arrives with knowledge the tutor has never seen, takes the model to another stand, looks back at how it stood a month ago, and the docs need a learner to show who is not the owner.

**Starting from zero is false.** A learner with a few years of school English knows most of the commonest few hundred words. A deck that opens *house* as new in its third week spends sittings on nothing, and the levels on Today start at a number that is a lie.

**Knowing is not answering.** "I know this word" said in an inventory is worth something, but it is not recall under a prompt: counted as an answer it would inflate the day's tally, close the day's room for new words after one evening, and - when FSRS is fitted to this learner - teach the model that recall is better than it is.

**The model has no outside.** Everything the learner has is rows in one SQLite file next to material rows. There is no way to hand it to another stand without the material, to read it with another tool, or to show a learner in the docs without showing the owner's.

**The past is not stored.** "How did I stand on 1 March" has no answer today but a backup of that date.

## Decision

**A word's card is the language's lemma.** A word card keys on `en:house` - the language and the lowercase lemma, as before - and is no longer assumed to have a pack's word behind it. The inventory makes cards for any lemma of the lexicon; a pack that brings the word later finds the card already there. Levels count every word card the lexicon ranks; the word counts on Today stay the pack's. A word card that holds the learner's knowledge is not an orphan when no pack teaches it; one never answered is.

**A declaration is a review with a source.** `review.source` is `asked` or `declared`. "I know it" is an *easy* applied through the same scheduler - FSRS opens a card answered easy as Review, so the word is sewn with the initial stability of an easy first answer, about fifteen days - and recorded with `source = 'declared'`. Declarations are not counted among the day's answers, do not spend the day's room for new words, carry no time and are to be left out when FSRS is fitted. A declared word comes due like any other, but a word is asked only in a sentence the learner has met, so it waits until the drill brings one.

**The inventory keeps both answers.** The `inventory` table holds one row per word asked - yes or no, and when - so the run never asks twice and can be taken back. A yes is also the declared review; the row is the question, the review is what it did. Taking a yes back deletes the declaration and plays the card's remaining history through the scheduler again; with nothing left the card is new again, or gone if only the declaration had made it. A yes the drill has answered since cannot be taken back.

**The learner model is a JSON file of format 1.** `hilvan export` and `GET /api/model` write it; `hilvan import` reads it. It holds what the learner has - languages, the packs it was learnt on, every answered card with its reviews and their typed turns, anchors by sentence text, own words, chosen voices, inventory answers - and nothing of the material, the audio cache or the sessions. It keys on what outlives a reload of a pack. It is strict: a field the format does not have is refused, and `format` changes when a file of this one would be misread.

**The schema is written by hand and published.** `docs/site/public/schema/learner-model-1.json`, served at `https://lacodda.github.io/hilvan/schema/learner-model-1.json`, is the contract; the file names it in `$schema`. The tests hold every export to it with `jsonschema` (a dev-dependency only), and hold import to refusing what it refuses; import also checks what a schema cannot say - nothing listed twice, every language named one the file lists.

**Import replaces, and keeps what it replaced.** An import replaces the whole learner model in one transaction and gives every loaded formula and word the model never answered a new card. On the command line it first writes the model being replaced to a file beside the database. There is no `--replace` flag and no merge: a merge of two schedules of the same card has no right answer.

**The past is a replay.** The model as it stood at a moment is the review log up to that moment played through the scheduler; the card rows are the last step of the same walk. Nothing is stored for it. A test plays every card of the demo and compares it with its stored state, so the replay is known to be the truth rather than assumed to be.

**The demo learner is made by the tutor.** `demo/learner.json` is three weeks of a fictional learner - an inventory on the first evening, a sitting most days, typed turns with slips, own words - produced by driving the real study code on the shipped pack with a fixed generator of its own (`cargo test --test demo -- --ignored`). It is built into the binary; `hilvan load-demo` loads it onto a tutor that holds no learner, moved by whole days so that its last evening is yesterday's.

## Consequences

- Every place that counts answers has to say which: today's tally, the day's room, pace (declarations carry no time), and later the fitting of FSRS.
- The inventory is honest only as far as the learner is; the schedule is what keeps it honest - a declared word that does not hold comes back loose the first time a sentence asks it.
- A changed scheduler would make old history replay differently. When parameters are fitted, the parameters in force at a moment become part of what a replay needs.
- The demo has to be made again when the pack renames what it learnt; a test fails until it is.
- A snapshot carries the voices chosen now: a choice keeps no date.
