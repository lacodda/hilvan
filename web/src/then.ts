/**
 * Then and now: where the learner stood on a day, beside where they stand.
 *
 * A day is the learner's own, on their own clock: "how I knew English on
 * 1 March" is the evening of the first, wherever they were. The server is
 * asked about the last moment of that day, as an instant.
 */

import type { Snapshot, Standings } from '@/api'
import { levelName } from '@/words'

/** A day as a date field holds it: YYYY-MM-DD. */
export type Day = string

/** The last moment of a day on this device's clock, as an ISO instant. */
export function endOf(day: Day): string {
  const [year, month, date] = day.split('-').map(Number)
  return new Date(year ?? 1970, (month ?? 1) - 1, date ?? 1, 23, 59, 59, 999).toISOString()
}

/** The day an instant falls on, on this device's clock. */
export function dayOf(instant: string | Date): Day {
  const at = typeof instant === 'string' ? new Date(instant) : instant
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`
}

/** A month before `now`: the day the screen looks back to first. */
export function firstLook(now: Date): Day {
  const monthAgo = new Date(now)
  monthAgo.setDate(monthAgo.getDate() - 30)
  return dayOf(monthAgo)
}

/**
 * What the screen opens on: a month ago - or, for a learner newer than that,
 * the evening of their first day, which the date field could not go before.
 */
export async function opening(ask: (at: string) => Promise<Standings>, now: Date): Promise<Standings> {
  const monthAgo = firstLook(now)
  const first = await ask(endOf(monthAgo))
  if (first.began === null) return first
  const began = dayOf(first.began)
  return began > monthAgo ? ask(endOf(began)) : first
}

/** One line of the comparison. */
export interface Line {
  label: string
  then: number
  now: number
}

/** What the comparison shows, in the order it shows it. */
export function lines(then: Snapshot, now: Snapshot): Line[] {
  const held = (snapshot: Snapshot) => snapshot.progress.produce.basted + snapshot.progress.produce.sewn
  return [
    { label: 'Formulas started', then: held(then), now: held(now) },
    { label: 'Formulas you can say', then: then.progress.produce.sewn, now: now.progress.produce.sewn },
    { label: 'Formulas you understand', then: then.progress.recognise.sewn, now: now.progress.recognise.sewn },
    ...now.words.levels.map((level, index) => ({
      label: `Words of the ${levelName(level.size)} level`,
      then: then.words.levels[index]?.sewn ?? 0,
      now: level.sewn,
    })),
    { label: 'Answers given', then: then.answers, now: now.answers },
  ]
}

/** How much a line moved, signed; nothing for no move. */
export function change(line: Line): string {
  const by = line.now - line.then
  if (by === 0) return ''
  return by > 0 ? `+${by}` : `-${-by}`
}
