# 0012 · Texts are sentences, coverage is shared arithmetic, and a pack glosses its language

Date: 2026-10-26. Status: accepted.

## Context

v0.9.0 brings the reader: a learner brings any text, sees how much of it is theirs, and takes the words they do not know into the deck. Each part has a cheap version that a later version would have to undo.

**Where a text's sentences live.** A text could keep its own copy of its sentences. Then a word taken from it would be met in a sentence nothing else knows: not spoken by the voice, which says only what the material holds; not an anchor, which points at a sentence; not shared with the pack's sentence of the same words.

**What is stored of the reading.** Every running word read into its lemmas could be kept in a table, so a list of texts is a query. Then the reading is a second truth of the lexicon, wrong as soon as the lexicon grows.

**Who counts the coverage.** The list is counted on the server; the page has to be counted again on the phone after every tap - a round trip per word on a train is no reader. Two implementations of "how much is yours" would disagree the first time one of them is changed.

**What a word means.** A pack glosses the 126 words its sentences teach. A text holds any of ten thousand. A tap that shows a meaning for one word in eighty is not a reader.

**What "in the deck" means for a word no pack teaches.** A word opens only once a sentence that holds it has been met (ADR 0006). A word of a text belongs to no formula, so no formula can meet it.

## Decision

**A text is sentences in the one table of sentences.** A text is a row and its sentences in order, each in its paragraph; the sentences are rows of `sentence`, kept once by what they say. The voice says them, an anchor can stand in them, and a sentence a text shares with a formula is that formula's sentence. A sentence is dropped only when nothing shows it: no formula, no text, no word taken from it.

**Nothing derived is stored.** A text is read through the lexicon on every request: tokens, the lemmas each can be, names. The lexicon's version is the reading's version. When a library grows past what that costs, a cache keyed on the binary's version is the answer, not a table.

**The rules of writing are data.** Clitics (*n't*, *'re*), spellings no rule splits (*can't*), and abbreviations that end no sentence (*Mr.*) are `lexicons/en/reading.toml`, built in with the lexicon. The code knows no English.

**Coverage is one piece of arithmetic in the core.** `hilvan-core` counts a text: a running word is held when every part of it is, a part when any lemma its spelling can be is; names are left out, spellings past the lexicon count and are never held. The server counts the list with it; the app calls it through the WebAssembly module after every tap. Holds are three: in the deck, basted, sewn - "yours" is the last two.

**A word is taken into the deck with its sentence.** `pick` is learner data keyed on the language's lemma, like a word's card: the word, the sentence, where it stands in it. The sentence becomes one the word has been met in - a view, `met`, is the one definition the queue, the day's debt and the word card read: a started formula's marked sentence, or a sentence the word was taken from. A taken word opens before the pack's new words, in the order taken, because the learner asked for it. It outlives the text it came from: removing a text keeps the sentence.

**"I know it" from the reader is a declaration.** The same declaration as the inventory's (ADR 0008), on one word or on every word of a paragraph not in hand. Taking it back plays the card's other answers again and lets the inventory ask the word again.

**A pack carries a glossary of its language.** A file next to the pack, `lemma<TAB>gloss` for every lemma of the lexicon, in the pack's native language; loaded and replaced with the pack. A word the pack teaches keeps its own gloss, the sense its sentences use. The glossary of `en-from-ru` was written once by a language model from the list of lemmas and is edited by hand from here; it is not regenerated, so a correction is never lost to a rebuild. Rejected: Wiktionary translations - a multi-gigabyte extract with gaps and senses in no particular order.

**The learner model file is format 3.** It carries the texts whole and the words taken from them, by the sentence's text; format 2 is refused.

## Consequences

- A sentence of a text has no translation. The word card says where the sentence came from instead, and the learner's understanding of it is the context.
- A spelling of two lemmas counts as held if either is, which can flatter a text by a word or two in a hundred; "I know the rest" declares the commoner lemma, which is wrong now and then for *left*. Telling them apart needs a tagger, and the honest fix is that, not a table.
- The glossary is as good as its review: a wrong gloss is wrong for everyone until someone corrects the line.
- Reading the whole library on every list grows with the library; books (v0.10) are where that is measured and a cache is added if it is needed.
- The reader is rhapsod's reading view, copied until the line's Reader block exists in dowel; moving onto the block is a later change to the screen only.
