---
title: Your first week
description: Loading the pack, signing in, what a sitting looks like, and what to expect as new formulas turn into basted ones and basted ones into sewn.
---

This is what actually happens once the tutor has material in it: loading a pack, signing in, and the shape of a sitting from the first day through the first week.

## Loading the pack

The tutor starts empty - no formulas, nothing to review. Load one:

```sh
hilvan load-pack packs/en-from-ru/pack.toml
```

This is a one-time step (or one you repeat only when the pack file itself changes - see [Formula packs](/hilvan/reference/packs/)). Running it again does nothing new, so it is safe to leave in the container's startup if that is convenient.

## Signing in

If the stand has [`HILVAN_PASSWORD_HASH`](/hilvan/reference/configuration/#the-password) set, the app asks for the password once; the session then lasts 90 days and every visit pushes that out, so this is not a daily step - and it survives the stand being updated. On an open stand - no password set - there is nothing to sign into at all.

## A sitting

Open the app and it shows you today's queue: whatever is due for review, and - if the day has room - one new formula. That "one new formula a day" limit is deliberate. A formula is a shape you assemble until it stops needing thought, and starting two on the same day means neither gets that.

**Reviews always come before the new formula.** If three formulas are due, you work through those first; the new one, when there is room for it, comes last. A day where you start something new while forgetting yesterday's is the thing hilvan is built to avoid.

For each formula, you see the pattern, an explanation in your own language, and worked examples. The drill substitutes different words into the same shape, so you are producing the sentence, not just recognizing it. When you answer, you grade yourself with one of four buttons:

- **Again** - nothing came. The formula goes back to the start.
- **Hard** - it came, but slowly and with effort.
- **Good** - it came.
- **Easy** - it came without thinking about it.

There is no wrong answer to grade yourself on beyond your own honesty - the four buttons only affect when the formula comes back, not whether you "pass".

### The three forms

Above the prompt sits a switch: **Say it**, **Deny it**, **Ask it** - the statement, the negation and the question of the same shape. Tap one and you drill that form: what is on screen is what gets graded, always. The small bar under each tab says where that form stands, so an untouched question is visible next to a statement you have sewn.

The card the queue offered stays unanswered when you switch away from it, and comes round again - nothing is lost by wandering into the question for a minute.

They are three separate formulas with three separate schedules, because they are learnt separately: the negation with `don't` really is its own thing to remember. The switch is what stops them feeling like three unrelated cards.

### The ladder

Once you have revealed an answer, **Run the ladder** puts the same formula through every person at once - I, you, he, she, we, they - with everything else held still. Say them straight through without stopping. It takes about a minute, and it is how the choice between *am*, *is* and *are* stops being a decision you make each time.

### Both directions

From the second day on, some cards come the other way round: the English is the prompt, and what you produce is the meaning. Understanding and saying are different skills - you will understand far more than you can say, which is normal - so each formula is scheduled twice, and the two never share a state. The Today screen shows the two columns side by side, **say** and **know**, and the gap between them is the honest picture.

A formula is only ever asked backwards after you have been shown it forwards, and the reverse card never uses up the day's one new formula.

### How long it took

Under the turn counter, a card that has been drilled a few times says how fast it usually comes - "usually 3.8s" - and after you grade it, whether this answer was quicker or slower than that. It is there to be noticed, nothing more: the schedule is not affected by how long you took. A formula that arrives after eight seconds of assembly is still on its way to being yours, and the number is how you watch that happen.

## New, basted, sewn

Every formula is in one of three states - the basting-stitch metaphor the product is named for:

- **New** - never answered.
- **Basted** - tacked in place: you have answered it, but the thread is still loose. It comes back soon, and a rough answer keeps it loose or a lapse can send it back here.
- **Sewn** - holding over long stretches of time. It still comes back, just rarely.

A formula moves from new to basted the first time you answer it, and from basted to sewn once it has held over long enough intervals. A forgotten formula that had been sewn goes back to basted, not to new - the stitch came loose, but you have not lost everything you knew about it.

## What the first week looks like

Day one is one formula: the first shape in the pack, with no reviews yet because nothing has been introduced before it. Day two adds the second formula, plus a review of the first - now due back. By day four or five, most sittings are mostly review, with the day's one new formula near the end. This is normal: reviews accumulate faster than they clear in the first week, because everything you have learned is still loose.

By the end of the week you will have somewhere around seven formulas started, most still basted, a few possibly sewn if you answered them well and their intervals happened to fall due again already. Do not expect "sewn" to dominate this early - stability builds over weeks, not days. What to watch instead is whether a sitting stays short: five to ten cards a day is the target shape, not fifty.

Because each formula is asked both ways, a sitting has roughly twice the cards it would otherwise - but the reverse ones are quicker, since recognising is the easier half. If sittings start feeling long, that is worth saying out loud rather than pushing through: the pace is a product decision, and it can change.
