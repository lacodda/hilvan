---
title: How hilvan teaches
description: Three mechanisms behind the tutor - comprehensible input, spaced retrieval, and production with feedback - and what the LLM's role actually is.
---

hilvan is built on three mechanisms that show up again and again in language acquisition research, not on a theory of its own. As of v0.1.0, the first of the three - production with feedback, through formulas and spaced retrieval - is what actually exists. Comprehensible input and the LLM-as-compiler are the vision the rest of the product builds toward, and this page says plainly which is which.

## Production with feedback - built

Recognising a form is easier than producing it, and producing it is what speaking and writing actually require. Grammar formulas are drilled by substitution - the same pattern, different words - so production practice happens on structures you can already half-manage, with feedback close enough in time to correct the model you are building rather than reinforce a mistake.

This is what v0.1.0 does: a [formula pack](/hilvan/reference/packs/) supplies the patterns, samples and slots, and the app drills them one substitution at a time.

v0.2.0 adds the other half of production. A shape is drilled in three forms - statement, negation, question - behind one switch on the card, because being able to say *I am tired* without being able to ask *Are you tired?* is half a formula. And the ladder runs one shape through every person in a minute, which is how the choice between *am*, *is* and *are* stops being a decision.

## Spaced retrieval - built

A formula drilled once is forgotten on a predictable schedule; recalling it right before it would have faded resets that schedule and pushes the next review further out. hilvan uses FSRS to decide what to review and when, so review time goes to what is about to be forgotten rather than what was already reviewed yesterday.

Every formula sits in one of three states - new, basted, sewn, the basting-stitch metaphor the product is named for - and reviews always come before new material. See [Your first week](/hilvan/guides/your-first-week/) for what this looks like day to day, and [ADR 0003](https://github.com/lacodda/hilvan/blob/main/docs/adr/0003-fsrs-scheduler.md) for why FSRS.

### Two directions, counted apart

Understanding a sentence and producing one are different skills, and recognition always runs ahead. From v0.2.0 each formula is scheduled twice - saying it, and understanding it - so the two are never averaged into a number that describes neither. The Today screen shows both columns, and the gap between them is the point: you will understand more than you can say, and seeing by how much is more useful than a single figure that hides it.

The recognising side of a shape opens only after the shape has been produced at least once, and never eats the day's one new formula.

### How fast it comes, not only whether it comes

A formula answered right, after eight seconds of assembling it, is not the same as one that simply arrives - and a memory model cannot see the difference, because both are "good". hilvan times answers and shows the median of the recent ones on the card. It is shown and never scheduled on: a home-made correction over FSRS would move every interval with no way to tell what moved it ([ADR 0005](https://github.com/lacodda/hilvan/blob/main/docs/adr/0005-forms-directions-and-pace.md)).

## Comprehensible input, slightly above level - coming

Material you can read or hear that sits just past what you already know teaches you the most per minute: known words and structures carry the meaning, and the few unknown pieces are learnable from context. Material that is too easy teaches nothing; material that is too hard teaches nothing either, because there is no foothold. This is the reason for the reader, still to come: it will measure how much of a real text you already know and target that narrow band on purpose, rather than handing you material written for nobody in particular.

## The LLM compiles material; it does not chat - coming

The plan is for an LLM to assemble sentences and audio lessons from your learner model - the formulas and words you already know - so that new material is reachable from where you actually are. A compiler with a grammar and a vocabulary as its build inputs, not a conversation partner improvising at whatever level it guesses you are at. Audio, voiced by a separate TTS provider, is next; formulas woven into full sentences follow; the reader and the compiler that ties it all together come after that.

## Coverage as the honest progress metric - coming

Once the reader exists, it will measure how much of a real text you already know, word by word and structure by structure, and report that coverage rather than a score. A text you cover at ninety percent is close to comprehensible input; a text you cover at forty percent is not yet worth your time. Coverage is meant to answer "am I ready for this" with a number instead of a guess, and to be the same number that tells you what to learn next in order to raise it. Today, the closest thing to a progress number is the count of formulas in each state on [`/api/today`](/hilvan/reference/api/#get-apitoday), told once per direction.
