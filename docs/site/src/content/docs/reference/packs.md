---
title: Formula packs
description: The pack.toml format - what a pack is, its shape, the parents a formula stands on, the answers it accepts, the words its sentences teach, the rules the loader enforces, and what loading does.
---

A pack is the material a learner starts from: grammar formulas, each with worked examples and the words to substitute into it, and the vocabulary those examples teach. A pack is **data, not code** - `pack.toml` is loaded into the database with `hilvan load-pack`, and the server itself never reads a pack file.

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
accept = ["<pronoun:short> <rest>."]   # optional: other answers just as right
explanation = "..."
order = 10
family = "be-present"      # optional: the shape this is one form of
form = "statement"          # optional: statement, negation or question

  [[formula.sample]]
  native = "<prompt in the learner's own language>"
  target = "I am at home."
  words = ["home"]
  accept = ["I'm at home."]   # optional: other spellings just as right

  [[formula.slot]]
  name = "pronoun"
  values = [
    { native = "<word in the learner's own language>", target = "I", be = "am", short = "I'm" },
    { native = "<word in the learner's own language>", target = "you", be = "are", short = "you're" },
  ]

  [[formula.slot]]
  name = "rest"
  takes = "noun"               # optional: the learner's own nouns go here too
  values = [{ native = "<...>", target = "at home" }]

[[formula]]
id = "be-present-negation"
order = 20
parents = ["be-present-statement"]   # optional: opens once these are sewn
# ...

[[word]]
lemma = "home"
gloss = "<what it means, in the learner's own language>"
```

| Field | Meaning |
| --- | --- |
| `id` | Stable pack id, matching the directory name (`packs/en-from-ru/` → `en-from-ru`). |
| `version` | Bumped whenever the contents change; what tells the loader "this pack changed" from "already have this". |
| `native` | The language prompts and explanations are written in (ISO 639-1, e.g. `ru`). |
| `target` | The language being learnt (ISO 639-1, e.g. `en`). |
| `[[formula]]` | One assembly pattern the learner builds sentences from. Repeated once per formula. |
| `[[word]]` | A word the worked examples teach - see [Words](#words). Repeated once per word. |

### A formula

| Field | Meaning |
| --- | --- |
| `id` | Stable across pack versions - the database keys on it, and cards and reviews stay attached to it even when the wording changes. |
| `name` | What the learner sees as the title. |
| `pattern` | The assembly shown compactly, with `<placeholder>` holes matching slot names. The scaffold on the card, not a sentence. |
| `say` | The sentence a substitution says, with `<slot>` and `<slot:form>` holes. Required when the formula has slots - see [The sentence it says](#the-sentence-it-says). |
| `accept` | Optional. More templates in the syntax of `say`, each another right answer - see [Answers it accepts](#answers-it-accepts). |
| `parents` | Optional. The ids of the formulas this one stands on; it opens only once all of them are sewn - see [Parents](#parents). |
| `explanation` | One paragraph, in the pack's native language. |
| `order` | The position this formula is introduced in - the pack decides the learning sequence, not the clock. |
| `family` | Optional. The shape this formula is one form of, so the card can offer a switch between them. |
| `form` | Optional, required alongside `family`. One of `statement`, `negation`, `question`. |
| `[[formula.sample]]` | A worked example: what a correct answer looks like. At least one per formula. |
| `[[formula.slot]]` | A hole in the pattern the drill substitutes into. Optional. |

A `sample` has `native` (the prompt), `target` (the answer), and optionally `words` - the lemmas of `target` it teaches - and `accept`, other spellings of `target` that are just as right. A `slot` has a `name` matching a `<placeholder>` in the pattern, `values` - a list of `{ native, target }` pairs the drill draws substitutions from - and optionally `takes`, the kind of the learner's [own words](#own-words) it takes as well.

A formula with no slot can still be recalled, just not drilled by substitution.

### Words

A word is never learnt alone: it enters the learner's deck only inside a sentence, once the formula that sentence belongs to has been drilled. So the pack says two things about words.

- **`words` on a sample** marks the lemmas the sentence teaches: `words = ["go", "doctor"]` on *She went to the doctor.* A mark is the lemma, not the spelling - *went* is found as a form of *go* through the [lexicon](/hilvan/reference/lexicon/) of the language. The words the formula itself is about - the pronoun, the *am* - are the formula's, and are left unmarked.
- **`[[word]]`** declares each word once, with its `gloss` in the pack's native language. How common the word is and how it sounds are not here: they belong to the language, and its lexicon says them.

A word marked in several sentences is one word with several contexts: one card, and the sentence it was first met in is the one it is heard in.

The same `target` in two formulas is one sentence. It has one translation and one set of words, and the loader holds the two formulas to that.

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

### Parents

A formula is built from others: *I would like* stands on *I want* and on the shape of a modal. `parents` names them, and a formula opens only once every parent's producing card is **sewn** - the next new formula is the first in the pack's `order` whose parents all are. The order is a preference; the tree is the law. A formula already started is never shut again because a parent came loose.

```toml
[[formula]]
id = "would-like"
order = 180
parents = ["want-to-verb", "modal-can"]
```

Every parent must be a formula of the same pack with a lower `order` than its child: a parent taught after its child would hold the child shut, and a loop would never open. Because a parent always comes first, a loop cannot be written down at all.

A formula with no parents opens as soon as the day has room for it. Keep a few such roots early in the sequence, so a day whose next formula waits on a loose parent can open another one instead.

### Answers it accepts

A said answer is typed and checked against what the formula says. *I'd like a coffee* and *I would like a coffee* are one answer, so a formula can list more templates that are just as right:

```toml
say = "<pronoun> would like <thing>."
accept = ["<pronoun>'d like <thing>."]
```

An accepted template follows the rules of `say` - it says every slot, and the forms it asks for are on the values. A contraction that changes with the person is a form like any other: `accept = ["<pronoun:short> <rest>."]` with `short = "she's"` on *she*. A fixed sentence lists its other spellings on the sample itself: `accept = ["I am not hungry."]` beside `target = "I'm not hungry."`. Case, punctuation and the kind of apostrophe never matter to the check; spelling does.

The check reads every word of the answer by where it came from. The template's own text, every form, and the value of a slot the formula agrees with - a slot some template asks a form of, like the pronoun that picks *am* or *is* - are the formula's; the plain value of any other slot is a word it carries. In a sample, the words marked in `words` are words and everything else is the formula's. That is how a wrong answer is told apart: a missing piece of the formula is a slip of the shape, missing words only are the word's failure, and the same words in another order are the order's.

### Own words

The learner can add nouns and verbs of their own life - *swim*, *a bicycle* - with what they mean. A slot that should take them says so:

```toml
  [[formula.slot]]
  name = "verb"
  takes = "verb"      # or "noun"
  values = [{ native = "<...>", target = "go" }]
```

A slot that takes own words must not be asked for any form of its value - neither by `say` nor by `accept` - because a word the learner typed carries none. The plain verb of *can*, *must*, *will* and *want to* takes verbs; the thing of *have got* and *would like* takes nouns. The drill draws an own word in about half the substitutions of such a slot.

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
- A value lacks a form `say` or an `accept` template asks of its slot, or carries a form no template asks for - the second is almost always a misspelt key.
- An `accept` template cannot be read, names a slot the formula does not have, leaves out a slot it does have, or repeats `say` or another template. A formula without `say` accepts nothing.
- A slot holds two values with the same `target` - a typed answer names the value it was asked with by what it says.
- A slot that `takes` own words is asked for a form of its value.
- A parent is not a formula of the pack, is named twice, or does not come before its child in `order`.
- A sample accepts an empty spelling, its own `target`, or the same spelling twice; or one sentence accepts different spellings in two formulas.
- A sample marks a word the pack declares no `[[word]]` for, marks the same word twice, or marks a word its sentence does not hold in any form.
- A `[[word]]` is in no sentence - it could never enter the deck - has no gloss, is more than one word, or shares its lemma with another, whatever the case.
- The same sentence is translated two ways, or marked with different words, in two formulas.

## Loading

```sh
hilvan load-pack packs/en-from-ru/pack.toml
```

Safe to run on every start of the container, or by hand whenever the file changes:

- **The same version loaded twice is a no-op.** Nothing is written, and the command says so.
- **A changed version (a bumped `version`) replaces the pack's formulas, slots and words**, but every card and review history stays - a card is keyed on the formula's `id` or the word's lemma, not on the pack's version, so correcting a formula's wording or a word's gloss does not reset a month of learning it.
- **A sentence is kept by what it says.** A corrected translation leaves it the same sentence, and a word goes on being heard in it. A sentence no formula shows any more is dropped, and a word that was heard in it is heard in the first sentence it is met in until it is answered again.
- **A formula or a word removed from the pack is not deleted.** Its card is kept, and the command reports it as an orphan so the removal can be confirmed as intentional rather than silently losing the learner's history:

  ```
  note: 1 card(s) belong to formulas or words no pack holds any more, and were kept: have-got
  ```

- One card is created per formula per direction, and per word, the first time it is seen; existing cards are never touched by a reload.

Loading is a separate command rather than something the server does at startup on its own: the learner decides when their material changes, and a server that silently rewrites the pack on every restart makes an editing mistake in the pack invisible.
