/**
 * Words: where one stands in its sentence, and how common it is, said the
 * way a learner says it.
 */

import type { Level, Word } from '@/api'

/** A sentence cut around the word it holds. */
export interface Marked {
  before: string
  word: string
  after: string
}

/**
 * The sentence, cut around the word that starts at `start`.
 *
 * The server counts in characters - code points - and a JavaScript string
 * indexes by UTF-16 unit, which part company at the first character outside
 * the basic plane; hence `Array.from`. When the word is not where it is said
 * to be - a sentence edited by hand - it is looked for by its spelling, and
 * a sentence that does not hold it at all is shown unmarked rather than
 * marked in the wrong place.
 */
export function mark(text: string, start: number, form: string): Marked {
  const chars = Array.from(text)
  const length = Array.from(form).length
  const at = chars.slice(start, start + length).join('') === form ? start : findForm(chars, form)
  if (at < 0) return { before: text, word: '', after: '' }
  return {
    before: chars.slice(0, at).join(''),
    word: chars.slice(at, at + length).join(''),
    after: chars.slice(at + length).join(''),
  }
}

function findForm(chars: string[], form: string): number {
  const lower = chars.join('').toLowerCase()
  const index = lower.indexOf(form.toLowerCase())
  return index < 0 ? -1 : Array.from(lower.slice(0, index)).length
}

/** A level's short name: 1000 is "1k". */
export function levelName(size: number): string {
  return size % 1000 === 0 ? `${size / 1000}k` : String(size)
}

/** Where a level stands, in a line: "12 sewn, 30 basted of 1000". */
export function levelLine(level: Level): string {
  const parts = [`${level.sewn} sewn`]
  if (level.basted > 0) parts.push(`${level.basted} basted`)
  return `${parts.join(', ')} of ${level.size}`
}

/**
 * How common a word is, in words: the level it belongs to, which is what the
 * learner aims at, rather than a rank nobody can feel.
 */
export function commonness(word: Pick<Word, 'level' | 'rank'>): string | null {
  if (word.level !== null) return `among the ${word.level} commonest words`
  if (word.rank !== null) return 'rarer than any level'
  return null
}
