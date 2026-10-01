# 0006 · Words live in sentences, and a lexicon ranks them

Date: 2026-10-01. Status: accepted.

## Context

v0.4.0 brings words into the learner model, and three questions come with them, each with a cheap answer that would have to be undone by the reader (v0.9) or the inventory of the first thousand words (v0.6).

**What a word is ranked by.** Levels - the commonest 1000, 2000, 5000 lemmas - need a frequency list of lemmas, not of spellings, deep enough to reach past 5k, under a licence that lets it sit in a public repository. The usual names fail one test each: COCA's licence forbids sharing its ranks; SUBTLEX-US is subtitles only, with its licence stated by a mirror rather than its authors; the NGSL is lemmatised and open but stops at 2,800. wordfreq (CC BY-SA 4.0) is broad and covers Spanish by the same method, but counts spellings: *left* is the direction and the past of *leave*, *tired* an adjective and the past of *tire*, and a list of spellings cannot tell which.

**What a sentence is.** A worked example was a row of its formula, rewritten on every reload. A word is learnt inside a sentence and heard in one of them every time it comes back, so the sentence needs an identity that survives a reload with a corrected translation.

**Where word data lives.** How common a word is and how it sounds belong to the language; what it means belongs to a pair of languages; how well the learner knows it belongs to the learner.

## Decision

**A lexicon per language, generated and built into the binary.** `tools/lexicon/build.py` combines wordfreq (frequencies), the English Speller Database (lemmas, forms, which spellings are names), UD English-EWT (which lemma a shared spelling was each time, hand-annotated - this is what splits *left* and keeps *john* out of the levels) and CMUdict (transcriptions) into `lexicons/en/lexicon.tsv`: ten thousand lemmas with rank, IPA and forms, CC BY-SA 4.0. `src/lexicon.rs` embeds it. It is not loaded into the database: one truth, and the version of hilvan is the version of its lexicon. Rejected: a hand-written table of homograph splits, which fixes cases where the treebank fixes the class.

**A sentence is a row identified by its language and text**, with translations in their own table and formulas linking to sentences. Two formulas showing the same example share one sentence; a reload upserts by text, so an anchor survives a corrected translation, and a sentence no formula shows is dropped with the anchors in it.

**The pack marks words; the lexicon checks the marks.** A sample lists the lemmas it teaches, each declared once with a gloss; the loader finds every mark in its sentence through the lexicon's forms, or refuses the pack. Function words stay unmarked - they are the formulas' business - which is why marking is the author's choice and not a tokenizer's.

**One card per word**, kind `word`, keyed `en:doctor`. It opens only once a sentence that holds it has been met, commonest first, at most ten a day; the first answer fixes the anchor, stored as learner state.

## Consequences

- **The reader of v0.9 has its lemmatiser.** The forms that check a pack's marks are the forms that will map a text's spellings to lemmas, and the ranks are what coverage is measured against.
- **Two licences in one repository.** The code is MIT; `lexicons/en` is CC BY-SA 4.0 with its attribution beside it. The builder is Python and only needed to regenerate the file.
- **The levels are the language's, not the pack's.** The bar on Today counts sewn words out of a thousand of the language, so it starts near zero and moves slowly. That is the honest number; a bar out of the pack's 126 words would fill and mean nothing.
- **Known artefacts stay visible.** wordfreq's sources run to 2021 and the treebank is older, so a spelling that became a name lately (*trump*) keeps its frequency. They sit outside what a pack teaches and are documented rather than patched.
- **A word is recognised, not produced.** Its card asks what it means in its sentence. Producing words is what every formula drill already does; a second, producing card per word would schedule the same skill twice.
