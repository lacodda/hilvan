---
title: The shelf
description: The open books and articles hilvan offers to read next - where they come from, how each is placed at a level and estimated for the learner before it is fetched, and how the profiles are measured.
---

The shelf is a list of open texts for a learner to read next: books of [Project Gutenberg](https://www.gutenberg.org) - public domain in the United States - and articles of the [Simple English Wikipedia](https://simple.wikipedia.org), written for readers still learning English. It is built into the server, like the [lexicon](/hilvan/reference/lexicon/): nothing of the texts is kept, only what they are and how their words fall in the frequency list. Taking an item fetches it from its source; from then on it is a text of the learner's, counted word by word.

![The shelf: articles of the Simple English Wikipedia, each with about how much of it is yours and the lexicon it needs](/hilvan/screenshots/shelf.png)

## A profile

Every item has a profile, measured from its text with the lexicon: how many of its running words fall in each band of the frequency list.

| Band | The lemmas ranked |
| --- | --- |
| 1-5 | 1-100, 101-200, 201-300, 301-500, 501-750 |
| 6-8 | 751-1000, 1001-1500, 1501-2000 |
| 9-14 | 2001-3000, 3001-4000, 4001-5000, 5001-6500, 6501-8000, 8001-10,000 |
| beyond | spellings the lexicon does not have |

A running word falls in the band of its rarest part - *don't* in the rarer of *do* and *not* - each part read as its commonest lemma, as "I know the rest" reads it. Names are left out, as the reader's coverage leaves them.

## Level and estimate

The **level** is the size of lexicon - the commonest 1000, 2000, 3000, 5000, 8000 or 10,000 lemmas - that holds 95% of the running words: the share at which a reader reads on their own. A spelling past the lexicon is always outside it. Most novels need 8000, children's books 5000, articles of the Simple English Wikipedia 3000 to 8000.

The **estimate** is how much of the item is the learner's, before it is fetched: each band weighed by how many of the item's running words fall in it and how many of the band's lemmas the learner holds, sewn or being learnt. On the demo learner it came within a point of the exact count, always a little under it - inside a band, the lemmas a learner knows are the commoner ones, and those are the commoner in a text too.

## What to read next

The [list of texts](/hilvan/reference/api/#get-apitexts) offers one thing to read next: an unfinished text at the chapter the learner is in, or a shelf item not taken yet, whichever stands nearest the window of **95 to 98 percent** of its words being the learner's. Below the window counts twice as far as above it. A learner below that line everywhere is offered the nearest from below: every word taken into the deck or swept as known moves it closer.

## Measuring

The catalogue, [`shelves/en/catalogue.toml`](https://github.com/lacodda/hilvan/blob/main/shelves/en/catalogue.toml), is written by hand. The profiles, [`shelves/en/profiles.tsv`](https://github.com/lacodda/hilvan/blob/main/shelves/en/profiles.tsv), are measured by fetching every item through the reader's own import:

```console
$ cargo test --test shelf -- --ignored --nocapture
```

The profiles' first line names the lexicon and the bands they were measured with, and a test holds them to the lexicon built in: a change of the lexicon, its rules of reading or the bands fails the gate until the shelf is measured again.
