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

If the stand has [`HILVAN_PASSWORD`](/hilvan/reference/configuration/#the-password) set, the app asks for it once; the session then lasts 30 days, so this is not a daily step. On an open stand - no password set - there is nothing to sign into at all.

## A sitting

Open the app and it shows you today's queue: whatever is due for review, and - if the day has room - one new formula. That "one new formula a day" limit is deliberate. A formula is a shape you assemble until it stops needing thought, and starting two on the same day means neither gets that.

**Reviews always come before the new formula.** If three formulas are due, you work through those first; the new one, when there is room for it, comes last. A day where you start something new while forgetting yesterday's is the thing hilvan is built to avoid.

For each formula, you see the pattern, an explanation in your own language, and worked examples. The drill substitutes different words into the same shape, so you are producing the sentence, not just recognizing it. When you answer, you grade yourself with one of four buttons:

- **Again** - nothing came. The formula goes back to the start.
- **Hard** - it came, but slowly and with effort.
- **Good** - it came.
- **Easy** - it came without thinking about it.

There is no wrong answer to grade yourself on beyond your own honesty - the four buttons only affect when the formula comes back, not whether you "pass".

## New, basted, sewn

Every formula is in one of three states - the basting-stitch metaphor the product is named for:

- **New** - never answered.
- **Basted** - tacked in place: you have answered it, but the thread is still loose. It comes back soon, and a rough answer keeps it loose or a lapse can send it back here.
- **Sewn** - holding over long stretches of time. It still comes back, just rarely.

A formula moves from new to basted the first time you answer it, and from basted to sewn once it has held over long enough intervals. A forgotten formula that had been sewn goes back to basted, not to new - the stitch came loose, but you have not lost everything you knew about it.

## What the first week looks like

Day one is one formula: the first shape in the pack, with no reviews yet because nothing has been introduced before it. Day two adds the second formula, plus a review of the first - now due back. By day four or five, most sittings are mostly review, with the day's one new formula near the end. This is normal: reviews accumulate faster than they clear in the first week, because everything you have learned is still loose.

By the end of the week you will have somewhere around seven formulas started, most still basted, a few possibly sewn if you answered them well and their intervals happened to fall due again already. Do not expect "sewn" to dominate this early - stability builds over weeks, not days. What to watch instead is whether a sitting stays short: five to ten formulas a day is the target shape, not fifty.
