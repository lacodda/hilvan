/**
 * The schedule living with the learner's life, said in sentences: a break,
 * an absence that could be one, a day that began owing, what slipped.
 *
 * Days are the learner's own, on this device's clock, the way Then and now
 * reads them.
 */

import type { Break, Note, Room, Slip } from '@/api'
import type { Day } from '@/then'

const weekdays = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'] as const
const months = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const

/** A moment as a day a person says: "Friday 11 September". */
export function dayName(moment: string | Date): string {
  const at = typeof moment === 'string' ? new Date(moment) : moment
  return `${weekdays[at.getDay()]} ${at.getDate()} ${months[at.getMonth()]}`
}

/** The first moment of a day on this device's clock, as an ISO instant: the morning one is back on. */
export function startOf(day: Day): string {
  const [year, month, date] = day.split('-').map(Number)
  return new Date(year ?? 1970, (month ?? 1) - 1, date ?? 1).toISOString()
}

/** The break being taken, in a sentence. */
export function breakLine(taken: Break): string {
  const since = `On a break since ${dayName(taken.starts_at)}.`
  return taken.ends_at
    ? `${since} Back on ${dayName(taken.ends_at)} - nothing comes due until then.`
    : `${since} Nothing comes due until you are back.`
}

/** The offer to count an absence as a break. */
export function awayLine(since: string): string {
  return `You have been away since ${dayName(since)}. Count it as a break? What came due while you were away comes back over the days ahead instead of all of it today, and nothing is owed.`
}

/** What the day's debt did to its room for new things; null for a day that owes nothing. */
export function roomLine(room: Room): string | null {
  if (room.debt === 0) return null
  const left = `${room.debt} ${room.debt === 1 ? 'card was' : 'cards were'} left over from earlier days`
  if (room.words === 0) return `${left}, so nothing new opens today.`
  const usual = room.words + room.debt
  return `${left}, so ${room.words} new ${room.words === 1 ? 'word opens' : 'words open'} today instead of ${usual}.`
}

/** Names, quoted, joined the way a sentence lists them. */
function listed(names: readonly string[]): string {
  const quoted = names.map((name) => `"${name}"`)
  if (quoted.length <= 1) return quoted[0] ?? ''
  return `${quoted.slice(0, -1).join(', ')} and ${quoted.at(-1)!}`
}

/** Last week's slips, as the toast under the title "Last week" tells them. */
export function noteLine(note: Note): string {
  const more = note.slipped - note.names.length
  const names = more > 0 ? `${note.names.map((name) => `"${name}"`).join(', ')} and ${more} more` : listed(note.names)
  return `${note.slipped} slipped: ${names}. ${note.slipped === 1 ? 'It comes' : 'They come'} back sooner now.`
}

/** What a slip was, beside its name: which way a formula, what a word means. */
export function slipDetail(slip: Slip): string {
  const what = slip.kind === 'word' ? (slip.gloss ?? 'a word') : slip.direction === 'recognise' ? 'understanding it' : 'saying it'
  const from = slip.declared ? ' - you had said you knew it' : ''
  return `${what}${from}`
}

/** Where a slip stands now. */
export function slipNow(slip: Slip): string {
  switch (slip.stitch) {
    case 'sewn':
      return 'sewn again'
    case 'basted':
      return 'basted'
    case 'new':
      return 'not started'
  }
}
