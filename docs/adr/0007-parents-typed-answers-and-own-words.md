# 0007 · Formulas stand on parents, a said answer is typed and checked, and the learner brings words

Date: 2026-10-07. Status: accepted.

## Context

v0.5.0 closes the block of formulas with four things that each have a cheap version and an honest one.

**Parents.** "I would like" is built from "I want" and the shape of a modal. A list of formulas opened one a day in the pack's order teaches the shapes, but not that one stands on another, and it happily opens a shape whose foundation is still loose.

**Checking.** A learner who says the answer and then grades themselves on four buttons is grading their memory of what they said. Self-assessment is the right tool for understanding, which cannot be typed; for producing it is the weakest link, and it leaves no record of what actually went wrong.

**What went wrong.** A wrong sentence fails in different ways: the shape slips (*She are busy*), a word the shape carries does not come (*She is bisy*), or every word comes in the wrong order (*She is busy?* asked as a question). Grading all three as one failure of the formula marks the shape down for a missing noun and sends nothing about the noun anywhere.

**Variants.** *I'd like* and *I would like* are one answer. A checker that accepts only the spelling `say` renders marks a right answer wrong, and the obvious fix - a table of English contractions in the checker - puts the grammar of one language in the code.

**Own words.** The pack says *I can swim* because its author chose *swim*. The verbs and nouns of the learner's own life are remembered faster, and they have no glosses, no forms and no place in a pack.

## Decision

**A parent is a formula, and it comes earlier.** A formula names its `parents`; each must be a formula of the pack with a lower `order`, so a cycle is not something the validator looks for but something the data cannot say. A formula opens only once every parent's producing card is sewn. The next new formula is the first in the pack's order whose parents are all sewn - the order is a preference, the tree is the law - and a formula already started is never shut again because a parent came loose. Words are not parents: a word already opens only inside a sentence that has been met.

**Saying is typed; understanding is heard.** In the producing direction the learner types each turn, and the server checks it. The review carries the turns, the server checks them again and grades the formula itself: a slip of the shape - its own words, or their order - in half the turns or more is *again*, one slip is *hard*, none is *good*. *Easy* is granted only on the learner's word and only for a flawless round: whether it cost thought is the one thing typing does not show, and the time an answer took stays shown, never graded on (ADR 0005). Understanding keeps the four buttons.

**The check lives on the server, once.** Normalisation (case, punctuation, the kind of apostrophe), alignment (the longest common run of words), the choice of the variant a wrong answer is read against, and the kind of failure are one Rust module, `src/answer.rs`. The drill asks `POST /formulas/{id}/check` for each turn and shows what it says; the grade never trusts the client. The price is the offline queue of v0.8: a turn typed offline cannot be checked until it syncs, and that version decides how to show it.

**What failed is read from where each word came from.** `say` is rendered with the role of every character: the template's text, every `<slot:form>`, and the value of a slot the formula agrees with (a slot some template asks a form of - the pronoun that picks *am* or *is*) are the formula's; the plain value of any other slot is a word. In a fixed sample, the words the pack marked are words and the rest is the formula's. A missing formula word is a failure of the shape; missing words only, of the word; the same words in another order, of the order.

**The failure goes to the deck that owns it.** A turn that failed only on words leaves the formula's grade alone and records *again* on each missed word's card, when the pack teaches that word - opening it, if it was not open yet. A shape's failure sends no word anywhere: an empty answer says nothing about the words. Every turn is kept with its review - prompt, the answer it was read against, what was typed, the failure and the missed words - for the confusion pairs of v0.11 and the explanations of v0.15.

**Variants are templates in the pack.** `accept` on a formula lists more templates in the syntax of `say`; the contraction is a form on the value - `short = "she's"` - exactly as agreement already is. `accept` on a sample lists other spellings of its sentence. The code still knows no English.

**Own words are learner data in the slots that ask nothing of them.** The learner adds nouns and verbs with their meaning. A slot declares `takes = "noun"` or `"verb"`, and such a slot must not be asked for any form of its value - so a word typed in, which carries no forms, always renders, and the tutor needs no morphology to place it. Own words stand among the slot's values (marked `own`); the drill draws from them in half of that slot's substitutions, and they answer and fail like any value. A word the lexicon does not know is kept and flagged as a possible typo rather than refused, because a real word can lie past the lexicon's ten thousand.

## Consequences

- **Some days open a different formula than the pack's next one.** With *be-present-statement* only basted, the next root - *have-got*, *lets-verb* - opens instead of its children. Today names the formula next in line and the parents it waits on, so the day has a reason.
- **A sitting is slower per turn and more honest per grade.** Typing a sentence on a phone takes longer than saying it; the four turns are fewer judgements of more weight, and they are the learner's real mistakes rather than a guess at them.
- **Contractions are written into the pack.** v5 carries `short`, `be`, `do` and `got` forms on the values that need them and spellings on 53 samples. A new formula that teaches a contraction has to accept the other spelling or its learners will be marked wrong for being right; the shipped-pack test renders every accepted sentence and holds them to being sentences.
- **Own words reach 19 of the 30 formulas** - every plain verb slot, and the things of *have got* and *would like*. Slots that agree with their value (*there is/are*, *how much/many*, the inflected verbs of the past and the continuous) do not take them, and will not until a learner's word can carry forms.
- **A word missed in production is pulled back as a recognition card.** The word's card asks what it means in its sentence; a production miss is a reason to see it again sooner, which is what *again* on that card does. A producing card per word stays rejected (ADR 0006).
