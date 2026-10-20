/**
 * A sitting as the device holds it: what is left of today's queue once the
 * answers given here are taken off, the questions each formula will ask,
 * how much of it fits the minutes the learner has, and the sounds it will
 * play - fetched while the tutor is in reach, so that the sitting works on
 * a train.
 */

import { speechUrl, type Due, type Formula, type FormulaDue, type Usual } from '@/api'
import { nextPrompt, type Prompt, type Random } from '@/prompts'
import { sides, tempoFor, urlOf } from '@/speech'

/** How many turns one formula gets before it is graded. */
export const TURNS = 4

/** The card a queue item asks: `formula:<id>:<direction>` or `word:<id>`. */
export function cardOf(due: Due): string {
  return due.kind === 'formula' ? formulaCard(due.formula.id, due.direction) : wordCard(due.word.id)
}

export const formulaCard = (id: string, direction: FormulaDue['direction']): string => `formula:${id}:${direction}`
export const wordCard = (id: string): string => `word:${id}`

/**
 * The queue less the cards answered on this device since it was read: the
 * Today the device holds was read before those answers, and must not ask
 * them again while they are on their way, or after they land and before a
 * fresh Today counts them.
 */
export function unanswered(queue: readonly Due[], answered: ReadonlySet<string>): Due[] {
  return queue.filter((due) => !answered.has(cardOf(due)))
}

/**
 * How long a card is expected to take, in milliseconds: its own usual pace,
 * else the usual pace of its kind, else a first guess - a formula said is
 * four typed turns, one understood is four heard, a word is one look.
 */
export function expected(due: Due, usual: Usual): number {
  if (due.pace) return due.pace.typical_ms
  if (due.kind === 'word') return usual.word ?? FIRST_GUESS.word
  return due.direction === 'produce' ? (usual.produce ?? FIRST_GUESS.produce) : (usual.recognise ?? FIRST_GUESS.recognise)
}

const FIRST_GUESS = { produce: 90_000, recognise: 40_000, word: 15_000 }

/**
 * As much of the queue as fits `minutes`, from the front.
 *
 * From the front and nothing skipped: the queue is in the order things are
 * owed - reviews before anything new - and a short sitting that picked the
 * quick cards from further back would leave the slow, owed ones for a day
 * that never has the time either. One card at least, even a slow one: a
 * sitting of nothing is no sitting.
 */
export function fitting(queue: readonly Due[], minutes: number, usual: Usual): Due[] {
  const budget = minutes * 60_000
  const taken: Due[] = []
  let spent = 0
  for (const due of queue) {
    const cost = expected(due, usual)
    if (taken.length > 0 && spent + cost > budget) break
    taken.push(due)
    spent += cost
  }
  return taken
}

/** About how many minutes the queue takes, rounded up. */
export function minutesOf(queue: readonly Due[], usual: Usual): number {
  const total = queue.reduce((sum, due) => sum + expected(due, usual), 0)
  return Math.ceil(total / 60_000)
}

/**
 * The number on the app's icon: the cards waiting to come back. What came
 * back since the Today was read - its `coming` moments now past - counts
 * too, so a phone that has not reached the tutor for a day still shows the
 * day's reviews. New material is not counted: it is always there to start,
 * and a number that never falls to nothing stops meaning "come back".
 */
export function badgeCount(queue: readonly Due[], coming: readonly string[], now: number): number {
  const reviews = queue.filter((due) => !due.is_new).length
  const back = coming.filter((at) => Date.parse(at) <= now).length
  return reviews + back
}

/** The questions planned for each formula card of the queue, by card. */
export type Plan = Record<string, Prompt[]>

/**
 * The questions each formula of the queue will ask, drawn once and kept.
 *
 * Drawn when Today is read rather than turn by turn, so that the sounds of
 * every question can be fetched while the tutor is in reach: a sitting on a
 * train plays what was planned at home. A card keeps its questions until it
 * leaves the queue; one the pack no longer asks - a value gone from a slot,
 * a sentence gone from the formula - is drawn again.
 */
export function plan(queue: readonly Due[], kept: Plan, random: Random): Plan {
  const planned: Plan = {}
  for (const due of queue) {
    if (due.kind !== 'formula') continue
    const card = cardOf(due)
    const held = kept[card]
    planned[card] =
      held && held.length === TURNS && held.every((prompt) => stillAsks(due.formula, prompt))
        ? held
        : Array.from({ length: TURNS }, (_, turn) => nextPrompt(due.formula, turn, random))
  }
  return planned
}

/** Whether a question kept from an earlier read is one the formula still asks. */
export function stillAsks(formula: Formula, prompt: Prompt): boolean {
  const { asked } = prompt
  if ('sample' in asked) {
    const sample = formula.samples.find((candidate) => candidate.sentence === asked.sample)
    return sample !== undefined && sample.target === prompt.target && sample.native === prompt.native
  }
  const names = Object.keys(asked.values)
  return (
    formula.say !== null &&
    names.length === formula.slots.length &&
    formula.slots.every((slot) => slot.values.some((value) => value.target === asked.values[slot.name]))
  )
}

/**
 * Every sound a sitting plays, in the order it will be wanted: each
 * question and its answer, at the speed the card is said; the explanation
 * and the first examples of a formula being met; a word and the sentence it
 * is heard in. The forms of a shape the learner may switch to are left to
 * the network: they are a look sideways, not the sitting.
 */
export function soundsOf(queue: readonly Due[], planned: Plan): string[] {
  const urls: string[] = []
  for (const due of queue) {
    const tempo = tempoFor(due.stitch, due.is_new)
    if (due.kind === 'word') {
      const anchor = due.word.contexts[0]
      if (anchor) urls.push(speechUrl(due.word.languages.target, anchor.text, tempo))
      urls.push(speechUrl(due.word.languages.target, due.word.lemma, 'slow'))
      continue
    }
    const { formula } = due
    if (due.is_new) {
      urls.push(speechUrl(formula.languages.native, formula.explanation, 'normal'))
      for (const sample of formula.samples.slice(0, 3)) urls.push(speechUrl(formula.languages.target, sample.target, tempo))
    }
    for (const prompt of planned[cardOf(due)] ?? []) {
      const said = sides(prompt, due.direction, formula)
      for (const url of [urlOf(said.question, tempo), urlOf(said.answer, tempo)]) if (url) urls.push(url)
    }
  }
  return [...new Set(urls)]
}
