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
pattern = "<pronoun> + am/is/are + <rest>"
explanation = "..."           # one paragraph, in the native language
order = 10                    # the order formulas are introduced in

  [[formula.sample]]          # worked examples: what a correct answer looks like
  native = "Я дома."
  target = "I am at home."

  [[formula.slot]]            # the substitutions the drill draws from
  name = "pronoun"
  values = [
    { native = "я", target = "I" },
    { native = "ты", target = "you" },
  ]
```

Every formula needs at least one sample; slots are optional, but a formula
with no slot cannot be drilled by substitution, only recalled.
