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
explanation = "..."
order = 10

  [[formula.sample]]
  native = "<prompt in the learner's own language>"
  target = "I am at home."

  [[formula.slot]]
  name = "pronoun"
  values = [
    { native = "<word in the learner's own language>", target = "I" },
    { native = "<word in the learner's own language>", target = "you" },
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
| `pattern` | The assembly shown compactly, with `<placeholder>` holes matching slot names. |
| `explanation` | One paragraph, in the pack's native language. |
| `order` | The position this formula is introduced in - the pack decides the learning sequence, not the clock. |
| `[[formula.sample]]` | A worked example: what a correct answer looks like. At least one per formula. |
| `[[formula.slot]]` | A hole in the pattern the drill substitutes into. Optional. |

A `sample` has `native` (the prompt) and `target` (the answer). A `slot` has a `name` matching a `<placeholder>` in the pattern, and `values` - a list of `{ native, target }` pairs the drill draws substitutions from.

A formula with no slot can still be recalled, just not drilled by substitution.

## Validation

The loader rejects a pack outright rather than loading it partway - a half-loaded pack is a drill that silently skips a formula. `hilvan load-pack` fails with a message naming the problem when:

- The pack has no id, or holds no formulas at all.
- Two formulas share the same `id`.
- A formula has no sample.
- A slot has no values.
- Two slots in the same formula share a `name`.
- A slot's `name` has no matching `<name>` placeholder anywhere in the formula's `pattern` - a slot the pattern never mentions would be stored and never used.

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
