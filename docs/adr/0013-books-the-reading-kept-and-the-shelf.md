# 0013 · A book is a text with chapters, its reading is kept in memory, and the shelf is measured data

Date: 2026-11-01. Status: accepted.

## Context

v0.10.0 brings books: an `EPUB` or `FB2` file, a link to Project Gutenberg, an article of Wikipedia, any page; coverage by chapter; a shelf of open texts; and the tutor picking what to read next. Each part has a cheap version that a later version would have to undo.

**What a book is.** A book could be a table of its own whose chapters are texts. Then "a text" and "a chapter" are two names for one thing, and the place, the list, removing, the learner model file and "what to read next" each have to say which of the two they mean.

**What a chapter is.** Chapters could be the files of an `EPUB`. A publisher splits files by size as often as by chapter: one novel is sixty files of one chapter each, another is three files of twenty chapters.

**What reading a book costs.** ADR 0012 reads a text through the lexicon on every request and leaves a cache for books to measure. *Pride and Prejudice* - 130,000 running words - is read in 96 ms on a desktop, which is half a second or more on a Pi 4. A list of a library of books, and the next text picked across it, would take seconds on every request; and a chapter cannot be read alone, because a name is found through the whole book.

**Who chooses the next text.** Only a text already taken can be counted word by word. Fetching a library's books to count them would download megabytes on every visit to the shelf and lean on a library's servers.

**What a number is worth.** A text's id was the table's row number, given again after the last text was removed. The app keeps a text it read once under its address; a cache on the server would keep a reading under it too. Either would show one text as another.

## Decision

**A book is a text with chapters.** Every text has chapters - one for anything but a book - and a chapter is a title and the paragraph it starts at; the paragraphs run on through the whole text, and the place in it is still one paragraph. The list, the place, removing a text and the learner model file have one thing to talk about.

**Chapters are the source's structure.** An `EPUB`'s chapters are its table of contents - the `EPUB` 3 navigation or the `EPUB` 2 `NCX` map, not its list of printed pages - and a chapter starts where an entry points, at a file or at an element inside one. An `FB2`'s chapters are its sections. A chapter that reads nothing before the next one starts - a title page, a part that only holds chapters - gives its place to the next. Headings name chapters rather than stand as paragraphs. A book with no usable contents is a chapter a file.

**One tokenizer reads every markup.** html5gum, a tokenizer of HTML, reads the XHTML of a book, the XML that describes it, an `FB2` and a web page: it knows every entity a book writes without declaring, and recovers from markup no XML parser would take. What is not prose is a subtree stepped over by its own name. Encodings are `encoding_rs`'s: the byte order mark, what the sender or the text declares, UTF-8, and Windows-1252 for old files that are not UTF-8.

**The reading of a text is kept in memory while the server runs.** A text never changes once added and the lexicon is built into the binary, so the process is the reading's version: nothing to invalidate but a text removed. The reading keeps the text's distinct running words once - *Pride and Prejudice* is 3,544 of them - and each chapter as how many times each stands in it, so a chapter is counted by `hilvan-core::cover::weigh`, the arithmetic of `cover` weighed by those counts. The server reads every text when it starts, while it already answers, and reads a book on a thread for blocking work. A table of readings stays refused: it would be a second truth of the lexicon.

**A text's id is never given again.** The text table counts its ids up (`AUTOINCREMENT`), so an id names one text for the life of the database, for the server's memory and the app's.

**The shelf is measured data, built in like the lexicon.** A catalogue written by hand names the items - Gutenberg books and Simple English Wikipedia articles - and a profile measured from each item's text says how many of its running words fall in each of fourteen bands of the frequency list. A profile places an item at a level - the lexicon that holds 95% of it - and estimates how much of it is the learner's from the share of each band they hold. The profiles are measured by fetching every item through the reader's own import (`cargo test --test shelf -- --ignored`) and carry the fingerprint of the lexicon and the bands; a test fails when they no longer match.

**What to read next is the text nearest the window of 95 to 98 percent.** Every unfinished text, at the chapter the learner is in, and every shelf item not taken yet: the nearest to the window, below it counting twice as far as above it; between equals, the text being read, then the learner's own, then the middle of the window.

**The words taken go to Anki as text, not as a package.** A tab-separated file with Anki's import header, the note's id made of the word and its sentence's text: imported again, it updates its notes. An `.apkg` is a SQLite database of Anki's own schema, which changes between versions.

**The learner model file is format 4.** A text carries its chapters, its author and its source; format 3 is refused.

## Consequences

- An `EPUB` whose contents point at anchors that are not in its files reads as fewer, longer chapters. A table of contents that names illustrations among chapters, as some of Gutenberg's do, names the chapters with their captions.
- The first list after a restart waits for nothing: the library is read in the background. A text asked for before its turn is read on demand.
- The memory the readings take grows with the library: a novel is a few megabytes.
- The shelf's estimate runs a point or so under the exact count: inside a band, a learner holds the commoner lemmas, which are also the commoner in a text. A changed lexicon means measuring the shelf again, with the internet.
- A page with no `article` and no `main` is read whole, its menus' lines left out only when they are too short to be prose.
- An Anki note's sentence has no sound: the voice is the tutor's, and Anki would need the files.
