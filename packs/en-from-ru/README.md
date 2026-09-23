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
explanation = "..."           # one paragraph, in the native language
order = 10                    # the order formulas are introduced in
family = "be-present"          # optional: the shape this is one form of
form = "statement"             # optional: statement, negation or question

  [[formula.sample]]          # worked examples: what a correct answer looks like
  native = "Я дома."
  target = "I am at home."

  [[formula.slot]]            # the substitutions the drill draws from
  name = "pronoun"
  values = [
    { native = "я", target = "I", be = "am" },    # be: a form `say` agrees with
    { native = "ты", target = "you", be = "are" },
  ]
```

Every formula needs at least one sample; slots are optional, but a formula
with no slot cannot be drilled by substitution, only recalled. A formula with
slots needs `say`: the pattern is a scaffold, and `say` is the sentence the
drill answers with and the tutor reads aloud. Agreement lives on the value -
`be = "is"` on "he" - so the code knows nothing about English.

A statement, its negation and its question stay three formulas with three
schedules - the negation with `don't` really is a separate thing to
remember - and `family` is what lets the drill show them on one card behind a
switch. Both fields go together, and a formula that is nobody's negation
leaves them out.
