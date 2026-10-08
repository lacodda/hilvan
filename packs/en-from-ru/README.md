# Formula packs

A pack is the material a learner starts from: grammar formulas with worked
samples and the words to substitute into them. Packs are **data, not code** -
`pack.toml` is loaded into the database by `hilvan load-pack`, and loading the
same pack twice changes nothing.

The native language of the learner is a property of the pack, not of hilvan:
`en-from-ru` teaches English to a Russian speaker, and a later `es-from-ru` or
`en-from-es` is another file next to it, not another branch in the code.

## Shape

```toml
id = "en-from-ru"          # stable, matches the directory name
version = 1                 # bumped when the contents change
native = "ru"               # the language prompts are written in
target = "en"               # the language being learnt

[[formula]]
id = "be-present-statement"   # stable across versions; the database keys on it
name = "I am / you are"       # what the learner sees as the title
pattern = "<pronoun> + am/is/are + <rest>"   # the scaffold shown on the card
say = "<pronoun> <pronoun:be> <rest>."       # the sentence a substitution says
accept = ["<pronoun:short> <rest>."]         # optional: other answers just as right
explanation = "..."           # one paragraph, in the native language
order = 10                    # the order formulas are introduced in
parents = []                  # optional: formulas this one stands on, earlier in order
family = "be-present"          # optional: the shape this is one form of
form = "statement"             # optional: statement, negation or question

  [[formula.sample]]          # worked examples: what a correct answer looks like
  native = "Я дома."
  target = "I am at home."
  accept = ["I'm at home."]   # optional: other spellings just as right

  [[formula.slot]]            # the substitutions the drill draws from
  name = "pronoun"
  values = [
    { native = "я", target = "I", be = "am", short = "I'm" },    # be, short: forms the templates agree with
    { native = "ты", target = "you", be = "are", short = "you're" },
  ]

  [[formula.slot]]
  name = "rest"
  takes = "noun"              # optional: the learner's own nouns go here too
  values = [{ native = "дома", target = "at home" }]
```

Every formula needs at least one sample; slots are optional, but a formula
with no slot cannot be drilled by substitution, only recalled. A formula with
slots needs `say`: the pattern is a scaffold, and `say` is the sentence the
drill answers with and the tutor reads aloud. Agreement lives on the value -
`be = "is"` on "he" - so the code knows nothing about English.

A typed answer is checked against `say` and every template in `accept`, so a
formula that teaches a contraction accepts the other spelling as well: the
contraction is a form on the value (`short = "she's"`), and a sample lists its
other spellings in its own `accept`.

`parents` names the formulas this one is built from; it opens only once all of
them are sewn, and each must come earlier in `order`. A slot with `takes`
accepts the learner's own nouns or verbs, and so may not ask its value for any
form.

A statement, its negation and its question stay three formulas with three
schedules - the negation with `don't` really is a separate thing to
remember - and `family` is what lets the drill show them on one card behind a
switch. Both fields go together, and a formula that is nobody's negation
leaves them out.

## Words

The worked examples also teach vocabulary. A sample marks the lemmas it
carries, and each lemma is declared once at the end of the file with its
gloss:

```toml
  [[formula.sample]]
  native = "Она пошла к врачу."
  target = "She went to the doctor."
  words = ["go", "doctor"]        # lemmas: "went" is found as a form of go

[[word]]
lemma = "doctor"
gloss = "врач"
```

Mark the words a learner should come away with - nouns, verbs, adjectives,
adverbs - and leave the formula's own words unmarked: the pronoun and the
*am* are what the formula drills. A phrase (*make it*, *get up*) is not
marked word by word; collocations come in a later version. How common a
word is and how it sounds are not written here - the lexicon of the language
says them (`lexicons/en`).

This pack marks 126 words across its 150 sentences.
