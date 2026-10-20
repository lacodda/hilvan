/** What today's queue holds, said in a sentence or two. */

import type { Due, Next, Room } from '@/api'
import type { DeliveryState } from '@/delivery'

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

/**
 * The formula next in line, and what holds it: the tree made visible. A
 * formula waits for its parents to be sewn, or for a day that does not begin
 * owing ten cards, and saying which turns a day with nothing new into a day
 * with a reason.
 */
export function nextLine(next: Next | null, room: Room): string | null {
  if (!next) return null
  // Quoted: a formula's name is the pack's, and it can hold a dash of its own.
  if (next.waits_for.length === 0) {
    return room.formulas === 0 && room.debt > 0
      ? `Next: "${next.name}", on a day that begins with fewer cards left over.`
      : `Next: "${next.name}", on the next day with room for a new formula.`
  }
  const names = next.waits_for.map((parent) => `"${parent.name}"`)
  const list = names.length === 1 ? names[0]! : `${names.slice(0, -1).join(', ')} and ${names.at(-1)!}`
  return `Next: "${next.name}", once ${list} ${names.length === 1 ? 'is' : 'are'} sewn.`
}

// The interface is written in English, and its dates read in it.
const clock = new Intl.DateTimeFormat('en', { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })
const weekday = new Intl.DateTimeFormat('en', { weekday: 'long', day: 'numeric', month: 'long' })

/**
 * Said when Today is the copy kept on the device because the tutor is out
 * of reach: how old it is, so a learner who studies from it knows.
 */
export function asOfLine(asOf: string, now: Date = new Date()): string {
  const then = new Date(asOf)
  const time = clock.format(then)
  const sameDay = then.toDateString() === now.toDateString()
  const when = sameDay ? `at ${time}` : `on ${weekday.format(then)}, ${time}`
  return `Out of the tutor's reach - this is today as it stood ${when}. Answers wait on this device and go when it can be reached.`
}

/**
 * What the answers on this device are waiting for, if any: the tutor, a
 * sign-in, or nothing at all because the page will not keep them.
 */
export function waitingLine(state: Pick<DeliveryState, 'waiting' | 'unkept' | 'signIn' | 'refused'>): string | null {
  const lines: string[] = []
  if (state.waiting > 0) {
    const answers = state.waiting === 1 ? '1 answer on this device waits' : `${state.waiting} answers on this device wait`
    lines.push(state.signIn ? `${answers} for you to sign in again.` : `${answers} to reach the tutor.`)
  }
  if (state.unkept > 0) {
    lines.push('This browser will not keep them on the device: they are lost if the page is closed before they are sent.')
  }
  if (state.refused > 0) {
    lines.push(
      state.refused === 1
        ? 'The tutor refused one answer - its card is no longer in a pack.'
        : `The tutor refused ${state.refused} answers - their cards are no longer in a pack.`,
    )
  }
  return lines.length > 0 ? lines.join(' ') : null
}
