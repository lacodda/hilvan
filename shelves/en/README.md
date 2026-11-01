# The English shelf

`catalogue.toml` is the list of open texts hilvan offers an English learner to read next: books of [Project Gutenberg](https://www.gutenberg.org) and articles of the [Simple English Wikipedia](https://simple.wikipedia.org). It is written by hand and is hilvan's own (MIT).

`profiles.tsv` is measured from the texts: for every item, how many of its running words fall in each band of the frequency list of [`lexicons/en`](../../lexicons/en/README.md). Its first line names the lexicon and the bands it was measured with. Measure it again after changing the catalogue, the lexicon or the bands:

```console
$ cargo test --test shelf -- --ignored --nocapture
```

Both are built into the binary. No text of an item is kept here: an item is fetched from its source when the learner takes it.

## Sources

| Source | What it gives | Licence |
| --- | --- | --- |
| Project Gutenberg | The books, fetched as `EPUB` when taken | Public domain in the United States; the Project Gutenberg licence covers the files and their name, and the reader leaves its boilerplate out |
| Simple English Wikipedia | The articles, fetched as plain text when taken | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/); a text taken keeps the article's address as its source |

The numbers in `profiles.tsv` are counts of words, not text.
