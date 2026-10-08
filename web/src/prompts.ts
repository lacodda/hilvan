/**
 * What one turn of the drill asks.
 *
 * A formula is a shape with holes in it. The drill fills the holes from the
 * slot values and asks the learner to say the whole thing - which is the
 * point: you learn a formula by assembling it, not by recognising it.
 */

import type { Asked, Checked, Direction, Failure, Form, Formula, Pace, Rating, Slot, Stitch, Value } from '@/api'

/** One question: a prompt in the learner's language and the answer expected. */
export interface Prompt {
  /** What the learner reads. */
  native: string
  /** What they should have said. */
  target: string
  /** Where it came from, so the screen can say "an example" or "your own". */
  source: 'sample' | 'substitution'
  /** The question, as the server is told it when the answer is checked. */
  asked: Asked
}

/** A source of randomness, passed in so a test can hand over a fixed one. */
export type Random = () => number

function pick<T>(values: readonly T[], random: Random): T | undefined {
  if (values.length === 0) return undefined
  const index = Math.min(values.length - 1, Math.floor(random() * values.length))
  return values[index]
}

/**
 * The sentence `say` makes of the values chosen.
 *
 * `<slot>` is the value's word, `<slot:form>` a form the value carries - the
 * pronoun "he" carries be = "is" - and the first letter is raised, so a
 * question opens "Is he...". The server renders the same template the same
 * way (`src/say.rs`) and checks a sentence against it before speaking it, so
 * the two must agree to the letter.
 *
 * `null` when a slot has no value chosen or a value lacks a form: the pack
 * validator rules both out, so it can only be a database edited by hand.
 */
export function render(say: string, chosen: ReadonlyMap<string, Value>): string | null {
  let missing = false
  const filled = say.replace(/<([^>]*)>/g, (_, inside: string) => {
    const [slot, form] = inside.split(':').map((part) => part.trim())
    const value = chosen.get(slot ?? '')
    const text = value === undefined ? undefined : form === undefined ? value.target : value.forms[form]
    if (text === undefined) missing = true
    return text ?? ''
  })
  if (missing) return null
  const tidy = filled.split(' ').filter((word) => word !== '').join(' ')
  return tidy.charAt(0).toUpperCase() + tidy.slice(1)
}

/**
 * The value for one slot. A slot that takes the learner's own words draws
 * from them half the time and from the pack's the other half: the words of
 * one's own life are the point, and the pack's are the ones the formula's
 * examples were written around.
 */
function choose(slot: Slot, random: Random): Value | undefined {
  const own = slot.values.filter((value) => value.own)
  const pack = slot.values.filter((value) => !value.own)
  if (own.length > 0 && pack.length > 0) return pick(random() < 0.5 ? own : pack, random)
  return pick(slot.values, random)
}

/**
 * Picks a value for every slot and says the sentence.
 *
 * The prompt is the scaffold filled in the learner's own words - "я + am/is/are
 * + дома" - so the choice between am, is and are is left to the learner; the
 * answer is the sentence itself, "I am at home.", which is what gets checked,
 * heard and, later, typed.
 *
 * Returns `null` when the formula cannot be substituted - no slots, a slot
 * with no values, no `say` - and the caller falls back to a sample rather than
 * showing half a sentence.
 */
export function substitute(formula: Formula, random: Random): Prompt | null {
  if (formula.slots.length === 0 || formula.say === null) return null

  let native = formula.pattern
  const chosen = new Map<string, Value>()

  for (const slot of formula.slots) {
    const value = choose(slot, random)
    if (!value) return null
    chosen.set(slot.name, value)
    const hole = `<${slot.name}>`
    if (!native.includes(hole)) return null
    native = native.split(hole).join(value.native)
  }

  const target = render(formula.say, chosen)
  if (target === null) return null
  const values = Object.fromEntries([...chosen].map(([slot, value]) => [slot, value.target]))
  return { native, target, source: 'substitution', asked: { values } }
}

/**
 * The question for one turn.
 *
 * Every third turn is a worked sample rather than a substitution: the samples
 * are natural sentences an adult actually says, and a drill that is only
 * assembled fragments drifts away from the language.
 */
export function nextPrompt(formula: Formula, turn: number, random: Random): Prompt {
  const wantsSample = turn % 3 === 0
  const sample = formula.samples.length > 0 ? formula.samples[turn % formula.samples.length] : undefined

  if (wantsSample && sample) {
    return { native: sample.native, target: sample.target, source: 'sample', asked: { sample: sample.sentence } }
  }

  const substituted = substitute(formula, random)
  if (substituted) return substituted

  // A formula with no usable slots is drilled on its samples alone.
  if (sample) return { native: sample.native, target: sample.target, source: 'sample', asked: { sample: sample.sentence } }

  // Neither: the pack validator rejects this, so it can only happen to a
  // database edited by hand. Show the pattern rather than an empty card; the
  // server will refuse to check it, which is the truth about it.
  return { native: formula.pattern, target: formula.pattern, source: 'sample', asked: { values: {} } }
}

/**
 * The same question, asked the way this direction asks it.
 *
 * Producing reads the native prompt and says the English; recognising reads
 * the English and says what it means. One prompt, turned round - the two
 * directions must be the same sentence, or the learner is being asked two
 * different questions and told they are one.
 */
export function ask(prompt: Prompt, direction: Direction): Prompt {
  if (direction === 'produce') return prompt
  return { ...prompt, native: prompt.target, target: prompt.native }
}

/** What the learner is being asked to do, in the drill's own words. */
export function instruction(direction: Direction): string {
  return direction === 'produce' ? 'Type it in English' : 'What does it mean? Then look'
}

/** What a checked turn found, in a phrase. */
export function verdictLine(checked: Pick<Checked, 'correct' | 'failure' | 'missed'>): string {
  if (checked.correct) return 'Right.'
  return failureLine(checked.failure ?? 'formula', checked.missed)
}

function failureLine(failure: Failure, missed: readonly string[]): string {
  switch (failure) {
    case 'formula':
      return missed.length === 0 ? 'The shape slipped.' : `The shape slipped, and ${quoted(missed)} did not come.`
    case 'order':
      return 'The right words, in the wrong order.'
    case 'word':
      return missed.length === 0 ? 'A word did not come.' : `The shape came - ${quoted(missed)} did not.`
  }
}

function quoted(words: readonly string[]): string {
  const marked = words.map((word) => `"${word}"`)
  return marked.length === 1 ? marked[0]! : `${marked.slice(0, -1).join(', ')} and ${marked.at(-1)!}`
}

/** How the typed turns went, before they are graded. */
export function tally(checked: readonly Pick<Checked, 'correct'>[]): string {
  const right = checked.filter((turn) => turn.correct).length
  if (right === checked.length) return checked.length === 1 ? 'Right.' : `All ${checked.length} right.`
  return `${right} of ${checked.length} right.`
}

/** The grade the turns earned, said after it is recorded. */
export function gradeLine(rating: Rating, words: readonly string[]): string {
  const grade = {
    again: 'Again - the shape is not there yet.',
    hard: 'Hard - it came, with a slip.',
    good: 'Good.',
    easy: 'Easy.',
  }[rating]
  if (words.length === 0) return grade
  const lemmas = words.map((word) => word.replace(/^[a-z]+:/, ''))
  return `${grade} ${lemmas.length === 1 ? 'The word' : 'The words'} ${quoted(lemmas)} ${lemmas.length === 1 ? 'goes' : 'go'} back to your words.`
}

/**
 * One rung of the ladder: the same formula with one pronoun substituted.
 *
 * The ladder is Petrov's drill - one shape run through every person until the
 * choice of "am/is/are" stops being a decision. It is built from the pronoun
 * slot, so a formula without one has no ladder and the screen does not offer
 * it.
 */
export interface Rung {
  /** The pronoun, in the learner's own language. */
  native: string
  /** The whole assembled sentence. */
  target: string
}

/** The slot names a ladder can be built from, most specific first. */
const LADDER_SLOTS = ['pronoun', 'subject']

/**
 * Every person of one formula, in the pack's own order.
 *
 * The other slots are held still at their first value: the ladder is about
 * one thing changing, and a sentence where the noun moves too teaches
 * nothing about the person. Each rung is the whole sentence - "He is at
 * home." - so the ladder is something to say and to hear, not a scaffold.
 */
export function ladder(formula: Formula): Rung[] {
  const pronouns = LADDER_SLOTS.map((name) => formula.slots.find((slot) => slot.name === name)).find(Boolean)
  if (!pronouns || pronouns.values.length === 0 || formula.say === null) return []
  const say = formula.say

  const held = formula.slots.filter((slot: Slot) => slot.name !== pronouns.name)
  const rungs: Rung[] = []
  for (const value of pronouns.values) {
    const chosen = new Map<string, Value>([[pronouns.name, value]])
    for (const slot of held) {
      const first = slot.values[0]
      if (first) chosen.set(slot.name, first)
    }
    const target = render(say, chosen)
    if (target !== null) rungs.push({ native: value.native, target })
  }
  return rungs
}

/** One tab of the switch between the forms of a shape. */
export interface Tab {
  id: string
  form: Form
  /** Where that form stands. `null` for the form on screen: it is the card
      being answered, and where it stands is about to change. */
  stitch: Stitch | null
  /** Whether this is the form currently being drilled. */
  current: boolean
}

/** Statement, negation, question: the order they are learnt in. */
const FORM_ORDER: Record<Form, number> = { statement: 0, negation: 1, question: 2 }

/**
 * The tabs of the form switch, for the formula on screen.
 *
 * Built from the formula being drilled plus its sisters, so the tab marked
 * current is always the one whose answer will be graded. Empty when the
 * formula stands alone - there is nothing to switch between.
 */
export function tabs(formula: Formula): Tab[] {
  const all: { id: string; form: Form | null; stitch: Stitch | null }[] = [
    ...formula.sisters,
    { id: formula.id, form: formula.form, stitch: null },
  ]
  const found = all
    .filter((tab): tab is { id: string; form: Form; stitch: Stitch | null } => tab.form !== null)
    .sort((a, b) => FORM_ORDER[a.form] - FORM_ORDER[b.form])
    .map((tab) => ({ ...tab, current: tab.id === formula.id }))

  return found.length < 2 ? [] : found
}

/** The word on each tab: the name of the form, not an example of it. */
export const formLabels: Record<Form, string> = {
  statement: 'Say it',
  negation: 'Deny it',
  question: 'Ask it',
}

/** The four answers, in the order they are shown. */
export const ratings = [
  { rating: 'again', label: 'Again', hint: 'nothing came' },
  { rating: 'hard', label: 'Hard', hint: 'slowly, with effort' },
  { rating: 'good', label: 'Good', hint: 'it came' },
  { rating: 'easy', label: 'Easy', hint: 'without thinking' },
] as const

/** How long until a formula comes back, said the way a person would. */
export function whenBack(days: number): string {
  if (days <= 0) return 'later today'
  if (days === 1) return 'tomorrow'
  if (days < 30) return `in ${days} days`
  const months = Math.round(days / 30)
  return months === 1 ? 'in a month' : `in ${months} months`
}

/** A duration in seconds, short enough to read at a glance. */
export function seconds(ms: number): string {
  const value = ms / 1000
  return value < 10 ? `${value.toFixed(1)}s` : `${Math.round(value)}s`
}

/**
 * How this answer compared with the usual pace of the card.
 *
 * `null` when the difference is not worth a sentence: a quarter either way is
 * ordinary variation, and a screen that remarks on every answer stops being
 * read.
 */
export function paceLine(pace: Pace | null): string | null {
  if (!pace) return null
  const ratio = pace.last_ms / pace.typical_ms
  if (ratio > 1.25) return `${seconds(pace.last_ms)} - slower than your usual ${seconds(pace.typical_ms)}`
  if (ratio < 0.75) return `${seconds(pace.last_ms)} - quicker than your usual ${seconds(pace.typical_ms)}`
  return `${seconds(pace.last_ms)}, about your usual`
}

/** Where a card stands after an answer, in a phrase. */
export function stitchLine(stitch: Stitch): string {
  switch (stitch) {
    case 'new':
      return 'Still new.'
    case 'basted':
      return 'Basted - tacked in place.'
    case 'sewn':
      return 'Sewn.'
  }
}
