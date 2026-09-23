---
title: Formula packs
description: The pack.toml format - what a pack is, its shape, the rules the loader enforces, and what loading does.
---

A pack is the material a learner starts from: grammar formulas, each with worked examples and the words to substitute into it. A pack is **data, not code** - `pack.toml` is loaded into the database with `hilvan load-pack`, and the server itself never reads a pack file.

The native language of the learner is a property of the pack, not of hilvan. `packs/en-from-ru/pack.toml` teaches English to a Russian speaker; a later pack teaching Spanish to that same learner, or English to a speaker of some other language, is another file next to it - not a branch in the code. Adding a language is adding a file; see [How hilvan teaches](/hilvan/concepts/how-hilvan-teaches/) for why the product is built this way.

## Shape

```toml
id = "en-from-ru"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be-present-statement"
name = "I am / you are"
pattern = "<pronoun> + am/is/are + <rest>"
say = "<pronoun> <pronoun:be> <rest>."
explanation = "..."
order = 10
family = "be-present"      # optional: the shape this is one form of
form = "statement"          # optional: statement, negation or question

  [[formula.sample]]
  native = "<prompt in the learner's own language>"
  target = "I am at home."

  [[formula.slot]]
  name = "pronoun"
  values = [
    { native = "<word in the learner's own language>", target = "I", be = "am" },
    { native = "<word in the learner's own language>", target = "you", be = "are" },
  ]
```

| Field | Meaning |
| --- | --- |
| `id` | Stable pack id, matching the directory name (`packs/en-from-ru/` → `en-from-ru`). |
| `version` | Bumped whenever the contents change; what tells the loader "this pack changed" from "already have this". |
| `native` | The language prompts and explanations are written in (ISO 639-1, e.g. `ru`). |
| `target` | The language being learnt (ISO 639-1, e.g. `en`). |
| `[[formula]]` | One assembly pattern the learner builds sentences from. Repeated once per formula. |

### A formula

| Field | Meaning |
| --- | --- |
| `id` | Stable across pack versions - the database keys on it, and cards and reviews stay attached to it even when the wording changes. |
| `name` | What the learner sees as the title. |
| `pattern` | The assembly shown compactly, with `<placeholder>` holes matching slot names. The scaffold on the card, not a sentence. |
| `say` | The sentence a substitution says, with `<slot>` and `<slot:form>` holes. Required when the formula has slots - see [The sentence it says](#the-sentence-it-says). |
| `explanation` | One paragraph, in the pack's native language. |
| `order` | The position this formula is introduced in - the pack decides the learning sequence, not the clock. |
| `family` | Optional. The shape this formula is one form of, so the card can offer a switch between them. |
| `form` | Optional, required alongside `family`. One of `statement`, `negation`, `question`. |
| `[[formula.sample]]` | A worked example: what a correct answer looks like. At least one per formula. |
| `[[formula.slot]]` | A hole in the pattern the drill substitutes into. Optional. |

A `sample` has `native` (the prompt) and `target` (the answer). A `slot` has a `name` matching a `<placeholder>` in the pattern, and `values` - a list of `{ native, target }` pairs the drill draws substitutions from.

A formula with no slot can still be recalled, just not drilled by substitution.

### The sentence it says

The `pattern` is a scaffold - `<pronoun> + am/is/are + <rest>` - and filled in it would read *I + am/is/are + at home*: nothing anyone says, and an "answer" that shows all three choices instead of checking the one the formula teaches. `say` is the sentence the scaffold stands for, and it is what the drill shows as the answer and what the tutor reads aloud.

A hole in `say` is either a slot, filled with the value the drill picked, or a slot and a form, filled with a form **that value carries**. Agreement lives next to the word it belongs to:

```toml
say = "<pronoun:be> <pronoun> <rest>?"

  [[formula.slot]]
  name = "pronoun"
  values = [
    { native = "ты", target = "you", be = "are" },
    { native = "он", target = "he", be = "is" },
  ]
```

- A form can be a suffix: `want<pronoun:s> to <verb>.` with `s = "s"` on *he* and `s = ""` on *I* says *He wants to go* and *I want to go*.
- The first letter is raised, so `<pronoun:be>` opens a question as *Is* and *he* opens a statement as *He*.
- Every key on a value other than `native` and `target` is a form.

### Forms of a shape

A statement, its negation and its question are three formulas, not one: the negation with `don't` is genuinely a separate thing to remember, and each gets its own review schedule. `family` and `form` are what let the drill show the three on one card behind a switch, so a learner who can say *I am tired* is one tap from asking *Are you tired?*

```toml
[[formula]]
id = "be-present-statement"
family = "be-present"
form = "statement"
# ...

[[formula]]
id = "be-present-negation"
family = "be-present"
form = "negation"
# ...
```

Both fields are optional and go together. A formula that is nobody's negation - `let-s-verb`, `how-much-many` - leaves both out, and the drill shows it without a switch.

## Validation

The loader rejects a pack outright rather than loading it partway - a half-loaded pack is a drill that silently skips a formula. `hilvan load-pack` fails with a message naming the problem when:

- The pack has no id, or holds no formulas at all.
- Two formulas share the same `id`.
- A formula has no sample.
- A slot has no values.
- Two slots in the same formula share a `name`.
- A slot's `name` has no matching `<name>` placeholder anywhere in the formula's `pattern` - a slot the pattern never mentions would be stored and never used.
- A formula names a `form` but no `family`, or a `family` but no `form` - a form with nothing to belong to can never be switched to.
- Two formulas claim the same `form` of the same `family` - the switch would show one of them at random.
- A `family` holds a single formula - a switch with nothing to switch to, always a sister renamed or not written yet.
- A formula has slots but no `say` - its substitutions would have nothing to answer with but the scaffold.
- `say` names a slot the formula does not have, or leaves out a slot it does have - the answer would drop a word the prompt showed.
- A value lacks a form `say` asks of its slot, or carries a form `say` never asks for - the second is almost always a misspelt key.

## Loading

```sh
hilvan load-pack packs/en-from-ru/pack.toml
```

Safe to run on every start of the container, or by hand whenever the file changes:

- **The same version loaded twice is a no-op.** Nothing is written, and the command says so.
- **A changed version (a bumped `version`) replaces the pack's formulas, samples and slots**, but every card and review history stays - a card is keyed on the formula's `id` in `card.subject_id`, not on the pack's version, so correcting a formula's wording does not reset a month of drilling it.
- **A formula removed from the pack is not deleted.** Its card is kept, and the command reports it as an orphan so the removal can be confirmed as intentional rather than silently losing the learner's history:

  ```
  note: 1 card(s) belong to formulas no pack holds any more, and were kept: have-got
  ```

- One card is created per formula the first time it is seen; existing cards are never touched by a reload.

Loading is a separate command rather than something the server does at startup on its own: the learner decides when their material changes, and a server that silently rewrites the pack on every restart makes an editing mistake in the pack invisible.
