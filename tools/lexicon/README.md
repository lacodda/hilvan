# The lexicon builder

Writes `lexicons/en/lexicon.tsv` - the English lemmas hilvan ranks words by -
from four open sources. See [`lexicons/en/README.md`](../../lexicons/en/README.md)
for what each source contributes and the licence of the result.

```sh
python -m venv .venv && .venv/bin/pip install wordfreq==3.1.1
.venv/bin/python tools/lexicon/build.py              # writes lexicons/en/lexicon.tsv
.venv/bin/python tools/lexicon/build.py --report     # how the commonest shared spellings were split
```

The database, the treebank and the dictionary are downloaded at pinned commits
into `tools/lexicon/.cache/` on the first run. wordfreq's data is frozen (the
project is in sunset), so the same inputs give the same file: a change to
`lexicon.tsv` is a change to the builder.

Python is only needed to build the file. The tutor reads the result, built
into the binary by `src/lexicon.rs`, and a test there parses it on every run.
