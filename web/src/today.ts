/** What today's queue holds, said in a sentence or two. */

import type { Due } from '@/api'

/** "formula" or "formulas", "word" or "words". */
function counted(n: number, one: string): string {
  return `${n} ${n === 1 ? one : `${one}s`}`
}

function capitalise(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1)
}

/**
 * The queue in words: formulas first, because they come first, then words.
 *
 * Three kinds of formula turn are told apart, because they are different
 * work: one coming back to be said, one to be understood backwards (the
 * first time included - it is the formula just met, not a new one), and the
 * one new formula of the day. Words are told as coming back or new.
 */
export function describeQueue(queue: readonly Due[]): string {
  const formulas = queue.filter((due) => due.kind === 'formula')
  const words = queue.filter((due) => due.kind === 'word')
  const backwards = formulas.filter((due) => due.direction === 'recognise').length
  const fresh = formulas.some((due) => due.is_new && due.direction === 'produce')
  const comingBack = formulas.length - backwards - (fresh ? 1 : 0)

  const parts: string[] = []
  if (comingBack > 0) parts.push(`${counted(comingBack, 'formula')} to come back to`)
  if (backwards > 0) parts.push(parts.length > 0 ? `${backwards} to see backwards` : `${counted(backwards, 'formula')} to see backwards`)
  if (fresh) parts.push(parts.length > 0 ? 'then one new one' : 'one new formula to start')

  const wordsBack = words.filter((due) => !due.is_new).length
  const wordsNew = words.length - wordsBack
  const wordParts: string[] = []
  if (wordsBack > 0) wordParts.push(`${counted(wordsBack, 'word')} to come back to`)
  if (wordsNew > 0) wordParts.push(wordsBack > 0 ? `${wordsNew} new` : `${counted(wordsNew, 'new word')} from the sentences you have met`)

  const sentences: string[] = []
  if (parts.length > 0) sentences.push(`${capitalise(parts.join(', '))}.`)
  if (wordParts.length > 0) {
    const line = wordParts.join(' and ')
    sentences.push(parts.length > 0 ? `Then ${line}.` : `${capitalise(line)}.`)
  }
  return sentences.join(' ')
}
