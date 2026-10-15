---
title: Your first week
description: Loading the pack, signing in, what a sitting looks like, what happens when you miss days or go away, and what to expect as new formulas turn into basted ones and basted ones into sewn.
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

## What you already know

Before the first formula, the tutor can learn what you bring. **What you already know** on Today - **Word check** in its footer later - walks the commonest words of English one at a time and asks one thing of each: do you know it? Tap **I know it** if you would understand the word in a sentence, **No** if not; on a keyboard, right and left (or Y and N), and backspace takes the last answer back.

A word you know starts **sewn**, and the levels on Today count it at once. It is your word against the schedule, not a free pass: the first time a formula's sentence brings it, it is asked like any other word, and if it does not hold it comes loose. A word you do not know is never asked here again, and nothing about it changes. Neither answer counts among the day's answers, and neither takes a place from the day's new words.

The commonest thousand go by in an evening. The run goes on to the 5,000 level when you want it to; it is not something to finish before starting.

## A sitting

Open the app and it shows you today's queue: whatever is due for review, and - if the day has room - one new formula. That "one new formula a day" limit is deliberate. A formula is a shape you assemble until it stops needing thought, and starting two on the same day means neither gets that.

**Reviews always come before the new formula.** If three formulas are due, you work through those first; the new one, when there is room for it, comes last. A day where you start something new while forgetting yesterday's is the thing hilvan is built to avoid.

For each formula, you see the pattern, an explanation in your own language, and worked examples. The drill substitutes different words into the same shape, so you are producing the sentence, not just recognizing it - and you produce it by **typing** it.

Each turn, type the sentence and press **Check** (or **I don't know**, if nothing comes). The tutor says at once whether it is right, plays the sentence, and when it is not right, shows it with what did not come marked: a piece of the formula itself in one colour, a word the formula carries in another, and what you typed that it does not have struck through. Case and punctuation do not matter; spelling does. A contraction is as right as the full form - *I'm at home* and *I am at home* are one answer. Then **Say it and record** lets you hear yourself next to the voice.

After four turns the tutor grades the formula from what went wrong, not you:

- A slip of the shape - its own words, or their order - in half the turns or more: **again**, it comes back soon.
- One slip: **hard**.
- No slip: **good**. If every turn was right, you are asked one thing the tutor cannot see - whether it cost thought. **Easy** if it did not.

A word that did not come, when the shape did, is not held against the formula: the word goes back to your words, and comes back on its own card.

### Formulas that wait

Some formulas stand on others - *would like* on *want to*, the past of a question on the present one - and a formula opens only once the ones it stands on are sewn. When the next formula in the pack is still waiting, Today says so - "Next: *would like*, once *want to* is sewn" - and a formula that stands on nothing opens in its place.

### Your own words

**Your own words**, at the foot of Today, is where you add nouns and verbs from your own life: a verb in its plain form (*swim*), a noun as you would say it after *I have* (*a bicycle*), each with what it means. The drill puts them into the formulas that can take them - *can*, *must*, *will*, *want to* for verbs; *have got*, *would like* for nouns - in about half of their turns. A word hilvan does not know is kept, and marked in case it is a typo.

### The three forms

Above the prompt sits a switch: **Say it**, **Deny it**, **Ask it** - the statement, the negation and the question of the same shape. Tap one and you drill that form: what is on screen is what gets graded, always. The small bar under each tab says where that form stands, so an untouched question is visible next to a statement you have sewn.

The card the queue offered stays unanswered when you switch away from it, and comes round again - nothing is lost by wandering into the question for a minute.

They are three separate formulas with three separate schedules, because they are learnt separately: the negation with `don't` really is its own thing to remember. The switch is what stops them feeling like three unrelated cards.

### The ladder

Once you have revealed an answer, **Run the ladder** puts the same formula through every person at once - I, you, he, she, we, they - with everything else held still. Say them straight through without stopping. It takes about a minute, and it is how the choice between *am*, *is* and *are* stops being a decision you make each time.

### Both directions

From the second day on, some cards come the other way round: the English is the prompt, and what you produce is the meaning. Those you do not type: the sentence is played, you recall what it means, look, and grade yourself with four buttons - **Again** (nothing came), **Hard** (slowly, with effort), **Good** (it came), **Easy** (without thinking). They only decide when the card comes back. Understanding and saying are different skills - you will understand far more than you can say, which is normal - so each formula is scheduled twice, and the two never share a state. The Today screen shows the two columns side by side, **say** and **know**, and the gap between them is the honest picture.

A formula is only ever asked backwards after you have been shown it forwards, and the reverse card never uses up the day's one new formula.

### Words

After the formulas come the words. Every worked example of a formula carries a word or two - *home*, *doctor*, *ready* - and once you have drilled the formula, its words are yours to start: the next time Today is read, they wait after the formulas, the commonest first, at most ten new ones a day.

A new word is met before it is asked: the card says the word slowly, then the sentence you met it in, with the word marked, its transcription and its meaning. **Got it - ask me** takes the meaning away, plays the sentence again, and asks what the word means there. Show it, and grade it with the same four buttons.

The sentence it was first met in stays its sentence - you will hear the word in it every time it comes back. When the same word turns up in another formula's example, it is not a second card: the card lists that sentence under **Also met in**.

Under **Your words** on Today, three bars show the levels - the commonest 1,000, 2,000 and 5,000 words of English - and how many of yours are sewn in each. They start near zero and fill slowly; they are measured against the language, not against the pack.

### How firmly it holds

Under the name of a card you have answered before runs a thin bar: how long it holds, on a scale of hours, days, weeks, months and a year, with a hairline where each begins. It is FSRS's stability - the time until you would recall it nine times out of ten - drawn rather than printed, and said in words underneath: "Holds for weeks". After you grade it the bar shows what the answer did: the stretch it grew by bright, or the stretch it lost as a warning.

### How long it took

Under the turn counter, a card that has been drilled a few times says how fast it usually comes - "usually 3.8s" - and after you grade it, whether this answer was quicker or slower than that. It is there to be noticed, nothing more: the schedule is not affected by how long you took. A formula that arrives after eight seconds of assembly is still on its way to being yours, and the number is how you watch that happen.

## Missed days

A day you miss leaves its cards waiting, and they are asked first when you come back. What they also do is take places from the new material: every card left over from before yesterday takes one of the day's ten new words, and once all ten are taken the new formula waits too. Today says so - "3 cards were left over from earlier days, so 7 new words open today instead of 10" - and the next formula's line says it is waiting for a day that begins with fewer left over.

The count is made as the day begins, so clearing the leftovers does not open another round of new material the same day: a day of catching up stays a day of catching up, and the next one starts fresh. A card that came due less than a day ago is never counted - it may have come due an hour after yesterday's sitting.

## A break

**Take a break**, in the footer of Today, stops the schedule's clock while you are away: a holiday, a busy fortnight. Give the day you will be back, or leave it empty and tap **I'm back** on Today when you are. While the break lasts Today asks for nothing, and when it ends everything picks up where it was - the same cards in the same order at the same pace - instead of a fortnight's reviews landing on the first evening home. An answer given during a break ends it.

Memory does not stop for a holiday, and the tutor does not pretend it does: the first answer after a break counts every day since the one before, so some cards come back loose. That is what the first days back are for.

If you were away without saying so - three days or more since your last answer, with cards waiting - Today offers to count it as a break: **Count it as a break** stops the clock from your last answer to now, what was owed is owed no longer, and the toast that confirms it has an **Undo**. A weekend is not offered: what came due over two days is a sitting's worth, and it should be done.

## What slipped

A card slips when you had it - you came up with it on an earlier day - and then could not: the formula that was sewn and would not come, the word met last week and gone, the word you said you knew in the inventory and did not. Learning something new in a sitting, failing it and getting it right ten minutes later is not slipping.

**What slipped**, in the footer of Today, lists this week's and last week's, each with where it stands now. On the first sitting of a week - until your first answer of it - a note comes up by itself with last week's: how many slipped and the first few by name. There is nothing to do about them; they come back sooner on their own. A formula that slips week after week is worth a second look at its explanation.

## New, basted, sewn

Every formula is in one of three states - the basting-stitch metaphor the product is named for:

- **New** - never answered.
- **Basted** - tacked in place: you have answered it, but the thread is still loose. It comes back soon, and a rough answer keeps it loose or a lapse can send it back here.
- **Sewn** - holding over long stretches of time. It still comes back, just rarely.

A formula moves from new to basted the first time you answer it, and from basted to sewn once it has held over long enough intervals. A forgotten formula that had been sewn goes back to basted, not to new - the stitch came loose, but you have not lost everything you knew about it.

## Then and now

**Then and now**, in the footer of Today, sets a day of your choosing beside today: formulas started, formulas you can say and understand, the words of each level, the answers given. Nothing is stored for it - the tutor plays your answers up to that evening through the schedule again - so any day since the first answer can be looked at. The same screen downloads your [learner model](/hilvan/reference/learner-model/) as a file, as it stands or as it stood that day.

## What the first week looks like

Day one is one formula: the first shape in the pack, with no reviews yet because nothing has been introduced before it. Day two adds the second formula, plus a review of the first - now due back. By day four or five, most sittings are mostly review, with the day's one new formula near the end. This is normal: reviews accumulate faster than they clear in the first week, because everything you have learned is still loose.

By the end of the week you will have somewhere around seven formulas started, most still basted, a few possibly sewn if you answered them well and their intervals happened to fall due again already. Do not expect "sewn" to dominate this early - stability builds over weeks, not days. What to watch instead is whether a sitting stays short: five to ten cards a day is the target shape, not fifty.

Because each formula is asked both ways, a sitting has roughly twice the cards it would otherwise - but the reverse ones are quicker, since recognising is the easier half. Words add five or six cards on the day their formula is drilled, and the reviews of them after; a word card is the quickest card there is. If sittings start feeling long, that is worth saying out loud rather than pushing through: the pace is a product decision, and it can change.
