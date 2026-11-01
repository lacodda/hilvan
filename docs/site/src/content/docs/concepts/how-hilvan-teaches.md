---
title: How hilvan teaches
description: Three mechanisms behind the tutor - comprehensible input, spaced retrieval, and production with feedback - and what the LLM's role actually is.
---

hilvan is built on three mechanisms that show up again and again in language acquisition research, not on a theory of its own. As of v0.1.0, the first of the three - production with feedback, through formulas and spaced retrieval - is what actually exists. Comprehensible input and the LLM-as-compiler are the vision the rest of the product builds toward, and this page says plainly which is which.

## Production with feedback - built

Recognising a form is easier than producing it, and producing it is what speaking and writing actually require. Grammar formulas are drilled by substitution - the same pattern, different words - so production practice happens on structures you can already half-manage, with feedback close enough in time to correct the model you are building rather than reinforce a mistake.

This is what v0.1.0 does: a [formula pack](/hilvan/reference/packs/) supplies the patterns, samples and slots, and the app drills them one substitution at a time.

v0.2.0 adds the other half of production. A shape is drilled in three forms - statement, negation, question - behind one switch on the card, because being able to say *I am tired* without being able to ask *Are you tired?* is half a formula. And the ladder runs one shape through every person in a minute, which is how the choice between *am*, *is* and *are* stops being a decision.

From v0.5.0 the feedback is the machine's, not the learner's guess at it. A formula is said by typing it, and every turn is checked: right or not, and when not, *what* went wrong - the shape slipped (*She are busy*), a word it carries did not come (*She is bisy*), or every word came in the wrong order (*She is busy?* for a question). The three are mended apart: a slip of the shape grades the formula down, and a missing word goes back to that word's own card instead. *I'd like* and *I would like* are one answer, because the pack lists both. Understanding stays a listening skill, graded by the learner, because what you understood is not something to type.

The formulas also stand on each other now. *I would like* is built from *I want* and the shape of a modal, and it opens only once those are sewn: grammar goes up a tree rather than down a list, and a shape never opens on a loose foundation.

And the sentences can be about your life. The nouns and verbs you add yourself - *swim*, *a bicycle* - go into the formulas that can take them, in about half of their substitutions. A sentence you might actually say is remembered faster than one the pack's author might.

## Spaced retrieval - built

A formula drilled once is forgotten on a predictable schedule; recalling it right before it would have faded resets that schedule and pushes the next review further out. hilvan uses FSRS to decide what to review and when, so review time goes to what is about to be forgotten rather than what was already reviewed yesterday.

Every formula sits in one of three states - new, basted, sewn, the basting-stitch metaphor the product is named for - and reviews always come before new material. See [Your first week](/hilvan/guides/your-first-week/) for what this looks like day to day, and [ADR 0003](https://github.com/lacodda/hilvan/blob/main/docs/adr/0003-fsrs-scheduler.md) for why FSRS.

### Starting from what you know - built

A learner never starts from nothing, and a schedule that treats *house* as new in the third week wastes the sitting it spends on it. From v0.6.0 an inventory walks the commonest words and asks of each whether you know it. A yes is a declaration, not an answer: the word starts sewn, with the schedule an easy first answer would give it, and the first sentence that brings it is its test. Declarations are kept apart from answers everywhere answers are counted - the day's tally, the day's new words, and the fitting of the memory model to you when that comes - because "I know it" is worth something, but it is not recall under a prompt ([ADR 0008](https://github.com/lacodda/hilvan/blob/main/docs/adr/0008-declared-knowledge-and-the-learner-model-file.md)).

### A schedule that lives with you - built

A spaced-repetition schedule assumes you come every day, and most people who stop using one stop the same way: a few missed days, a wall of reviews on the way back, a second look at the wall, and no third. From v0.7.0 the schedule bends where a life does.

A day that begins with cards left over from earlier days opens less that is new - one new word fewer for each leftover, and no new formula once they have taken all ten places - so the way back is reviews, not reviews and a full day's new material on top. A break stops the schedule's clock: nothing comes due while you are away, and on the way back everything picks up where it was rather than all at once. An absence you did not announce can be counted as a break afterwards, in one tap. Memory itself is not paused - the memory model is told every day that passed, holiday included - only the tutor's claim on your time is ([ADR 0009](https://github.com/lacodda/hilvan/blob/main/docs/adr/0009-breaks-leftovers-and-what-slipped.md)).

What you had and lost is named: each week's slips - a card you came up with on one day and could not on a later one - are listed, and the first sitting of a week opens with last week's. How firmly each card holds is on the card as a bar from hours to a year, not as a number of days.

### Where the time is - built

The time a learner has is mostly in between: a ride, a queue, ten minutes before something else. From v0.8.0 a sitting can be as long as the time there is - a few minutes takes the front of the queue, reviews first, measured by how long each card usually takes - and it runs with the tutor out of reach. Each typed turn is checked on the phone by the same code the tutor grades with, and each answer waits on the phone and arrives later at the moment it was given, so a day underground is a day of study like any other ([ADR 0011](https://github.com/lacodda/hilvan/blob/main/docs/adr/0011-answers-from-the-road.md)).

### Two directions, counted apart

Understanding a sentence and producing one are different skills, and recognition always runs ahead. From v0.2.0 each formula is scheduled twice - saying it, and understanding it - so the two are never averaged into a number that describes neither. The Today screen shows both columns, and the gap between them is the point: you will understand more than you can say, and seeing by how much is more useful than a single figure that hides it.

The recognising side of a shape opens only after the shape has been produced at least once, and never eats the day's one new formula.

### Words, inside the sentences they were met in - built

From v0.4.0 the vocabulary grows out of the formulas rather than from a list. Every worked example marks the words it teaches, and a word enters the deck only once its sentence has been met - once the formula it belongs to has been drilled. It is then asked where it was met: the sentence is played, the word is marked in it, and the question is what it means there. One word met in three sentences is one card with three contexts, and the sentence it was first met in - its anchor - is the one it is heard in every time it comes back, so a word arrives with the same melody each time.

Words are ranked by how common they are in the language, and the commonest open first. The levels - the commonest 1,000, 2,000 and 5,000 lemmas - are what a learner aims at, and the Today screen counts the words in hand against them. See [Lexicon and levels](/hilvan/reference/lexicon/).

### How fast it comes, not only whether it comes

A formula answered right, after eight seconds of assembling it, is not the same as one that simply arrives - and a memory model cannot see the difference, because both are "good". hilvan times answers and shows the median of the recent ones on the card. It is shown and never scheduled on: a home-made correction over FSRS would move every interval with no way to tell what moved it ([ADR 0005](https://github.com/lacodda/hilvan/blob/main/docs/adr/0005-forms-directions-and-pace.md)).

## Comprehensible input, slightly above level - built in part

Material you can read or hear that sits just past what you already know teaches you the most per minute: known words and structures carry the meaning, and the few unknown pieces are learnable from context. Material that is too easy teaches nothing; material that is too hard teaches nothing either, because there is no foothold. From v0.9.0 the [reader](/hilvan/guides/reading/) measures how much of a real text you already know, and a word you do not goes into the deck with the sentence you met it in - learnt from the context that made it worth learning. From v0.10.0 the tutor picks that text on purpose: the one of your own - at the chapter you are in - or from an open [shelf](/hilvan/reference/shelf/) of books and articles whose share of words already yours stands nearest 95 to 98 percent, the band where a text reads on its own and still has words to give.

## The LLM compiles material; it does not chat - coming

The plan is for an LLM to assemble sentences and audio lessons from your learner model - the formulas and words you already know - so that new material is reachable from where you actually are. A compiler with a grammar and a vocabulary as its build inputs, not a conversation partner improvising at whatever level it guesses you are at. Audio, voiced by a separate TTS provider, is next; formulas woven into full sentences follow; the reader and the compiler that ties it all together come after that.

## Coverage as the honest progress metric - built

The reader measures how much of a real text you already know, word by word, and reports that coverage rather than a score: the share of its running words you hold, names left out, a word you put in the deck counted apart until you have answered it. A text you cover at ninety-five percent is comprehensible input; a text you cover at forty percent is not yet worth your time. A book is counted chapter by chapter too, so the hard chapter shows itself before you reach it. Coverage answers "am I ready for this" with a number instead of a guess, and every word you take from the text or say you know moves it on the spot. The number is one piece of arithmetic in the code the server and the phone share, so the list and the page never disagree ([ADR 0012](https://github.com/lacodda/hilvan/blob/main/docs/adr/0012-texts-coverage-and-the-glossary.md)). Structures are not counted yet: coverage is of words.
