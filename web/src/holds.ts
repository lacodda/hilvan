/**
 * How firmly a card holds, as a bar rather than a number.
 *
 * The number is FSRS's stability: the days until recall is predicted to fall
 * to 90%. A learner has no use for "17.3", and every use for "weeks, nearly
 * months": so the bar is drawn on a scale of hours, days, weeks, months and a
 * year, logarithmic because the difference between an hour and a day is
 * worth as much as the one between a month and a year, and said in those
 * words.
 */

import type { TrackSegment } from '@/components/ui/track'

/** A stretch of the scale, and what it is called. */
export interface Band {
  /** Days where it starts. */
  from: number
  /** Days where it ends. */
  to: number
  /** What "holds for ..." says when a card stands in it. */
  name: string
}

/** The scale, shortest first. The last runs to two years: the end of a bar is a place, not "forever". */
export const bands: readonly Band[] = [
  { from: 1 / 24, to: 1, name: 'hours' },
  { from: 1, to: 7, name: 'days' },
  { from: 7, to: 30, name: 'weeks' },
  { from: 30, to: 365, name: 'months' },
  { from: 365, to: 730, name: 'a year and more' },
]

/** Where `days` stands on the scale, in the scale's own units (log days), clamped to it. */
export function position(days: number): number {
  const first = bands[0]!
  const last = bands.at(-1)!
  const clamped = Math.min(Math.max(days, first.from), last.to)
  return Math.log(clamped)
}

/** The ends of the scale, in the same units as `position`. */
export const scale = { from: position(bands[0]!.from), to: position(bands.at(-1)!.to) }

/** The band a stability falls in; `null` for a card that holds nothing yet. */
export function band(days: number): Band | null {
  if (!(days > 0)) return null
  return bands.find((band) => days < band.to) ?? bands.at(-1)!
}

/** What a card's bar says in words. */
export function holdLine(days: number, was?: number): string {
  const now = band(days)
  if (!now) return 'Not held yet'
  const before = was === undefined ? null : band(was)
  if (!before || before === now) return `Holds for ${now.name}`
  return `Holds for ${now.name} now - it held for ${before.name}`
}

/**
 * The bar, as segments of a Track: the scale cut at every band's edge, so
 * the bands show as hairlines, and at the stability - and, after an answer,
 * at where it stood before.
 *
 * On the card the held part is the accent. After an answer the part held
 * before and after is dimmed, and the stretch between the two is what the
 * answer did: the accent when it grew, the warning when it fell.
 */
export function segments(days: number, was?: number): TrackSegment[] {
  const held = days > 0 ? position(days) : scale.from
  const before = was === undefined ? null : was > 0 ? position(was) : scale.from
  const low = before === null ? held : Math.min(held, before)
  const high = before === null ? held : Math.max(held, before)

  const cuts = [...new Set([scale.from, ...bands.map((band) => position(band.to)), low, high])].sort((a, b) => a - b)
  const pieces: TrackSegment[] = []
  for (let index = 0; index + 1 < cuts.length; index += 1) {
    const start = cuts[index]!
    const end = cuts[index + 1]!
    if (end <= start) continue
    const tone: TrackSegment['tone'] =
      end <= low ? (before === null ? 'accent' : 'past') : end <= high ? (before !== null && before > held ? 'warn' : 'accent') : 'idle'
    pieces.push({ key: `${start}`, start, end, tone })
  }
  return pieces
}
