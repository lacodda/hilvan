/** Pairs of words the learner confuses: the sentence cut at its gap, and how the pair came about, said. */

import type { Question, Side } from '@/api'

/**
 * The sentence cut around the word it asks. The server counts in
 * characters - code points - and a JavaScript string indexes by UTF-16
 * unit, hence `Array.from`.
 */
export function gap(question: Pick<Question, 'text' | 'start' | 'length'>): { before: string; word: string; after: string } {
  const chars = Array.from(question.text)
  return {
    before: chars.slice(0, question.start).join(''),
    word: chars.slice(question.start, question.start + question.length).join(''),
    after: chars.slice(question.start + question.length).join(''),
  }
}

/** "You typed *in* where *at* stood, twice." - which word came for which, from the learner's own turns. */
export function swapLine(words: readonly Side[]): string | null {
  const parts = words
    .map((side, index) => ({ side, other: words[1 - index] }))
    .filter(({ side, other }) => other !== undefined && side.typed_for_the_other > 0)
    .map(({ side, other }) => `${side.lemma} where ${other!.lemma} stood ${times(side.typed_for_the_other)}`)
  return parts.length > 0 ? `You typed ${parts.join(', and ')}.` : null
}

function times(n: number): string {
  if (n === 1) return 'once'
  if (n === 2) return 'twice'
  return `${n} times`
}
