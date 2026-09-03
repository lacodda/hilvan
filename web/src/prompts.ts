/**
 * What one turn of the drill asks.
 *
 * A formula is a shape with holes in it. The drill fills the holes from the
 * slot values and asks the learner to say the whole thing - which is the
 * point: you learn a formula by assembling it, not by recognising it.
 */

import type { Direction, Form, Formula, Pace, Sample, Slot, Stitch } from '@/api'

/** One question: a prompt in the learner's language and the answer expected. */
export interface Prompt {
  /** What the learner reads. */
  native: string
  /** What they should have said. */
  target: string
  /** Where it came from, so the screen can say "an example" or "your own". */
  source: 'sample' | 'substitution'
}

/** A source of randomness, passed in so a test can hand over a fixed one. */
export type Random = () => number

function pick<T>(values: readonly T[], random: Random): T | undefined {
  if (values.length === 0) return undefined
  const index = Math.min(values.length - 1, Math.floor(random() * values.length))
  return values[index]
}

/**
 * Fills every `<slot>` in a pattern with one of its values.
 *
 * Returns `null` when the pattern cannot be filled - a slot with no values,
 * or a pattern with no holes at all. The caller falls back to a sample rather
 * than showing a half-substituted string.
 */
export function substitute(formula: Formula, random: Random): Prompt | null {
  if (formula.slots.length === 0) return null

  let native = formula.pattern
  let target = formula.pattern
  const chosen: { slot: Slot; value: Sample }[] = []

  for (const slot of formula.slots) {
    const value = pick(slot.values, random)
    if (!value) return null
    chosen.push({ slot, value })
  }

  for (const { slot, value } of chosen) {
    const hole = `<${slot.name}>`
    if (!native.includes(hole)) return null
    native = native.split(hole).join(value.native)
    target = target.split(hole).join(value.target)
  }

  // The pattern also carries the scaffolding a learner has to choose between
  // ("am/is/are"), and that is deliberately left in the prompt: choosing the
  // right one is half of what the formula teaches.
  return { native, target, source: 'substitution' }
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
    return { native: sample.native, target: sample.target, source: 'sample' }
  }

  const substituted = substitute(formula, random)
  if (substituted) return substituted

  // A formula with no usable slots is drilled on its samples alone.
  if (sample) return { native: sample.native, target: sample.target, source: 'sample' }

  // Neither: the pack validator rejects this, so it can only happen to a
  // database edited by hand. Show the pattern rather than an empty card.
  return { native: formula.pattern, target: formula.pattern, source: 'sample' }
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
  return { native: prompt.target, target: prompt.native, source: prompt.source }
}

/** What the learner is being asked to do, in the drill's own words. */
export function instruction(direction: Direction): string {
  return direction === 'produce' ? 'Say it in English, then look' : 'What does it mean? Then look'
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
 * nothing about the person.
 */
export function ladder(formula: Formula): Rung[] {
  const pronouns = LADDER_SLOTS.map((name) => formula.slots.find((slot) => slot.name === name)).find(Boolean)
  if (!pronouns || pronouns.values.length === 0) return []

  const held = formula.slots.filter((slot) => slot.name !== pronouns.name)
  return pronouns.values.map((value) => {
    let target = formula.pattern.split(`<${pronouns.name}>`).join(value.target)
    for (const slot of held) {
      const first = slot.values[0]
      if (first) target = target.split(`<${slot.name}>`).join(first.target)
    }
    return { native: value.native, target }
  })
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
