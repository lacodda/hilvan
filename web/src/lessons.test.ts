import { describe, expect, it } from 'vitest'

import type { Stretch } from '@/api'
import { askedBy, clock, hear, minutesLine, nextStart, previousStart, stretchAt, type Listened } from '@/lessons'

/** A fresh phrase heard and then asked, a phrase asked, and the fresh one asked again. */
const chapters: Stretch[] = [
  { phrase: 0, turn: 'heard', start_ms: 0, end_ms: 9_000 },
  { phrase: 1, turn: 'asked', start_ms: 10_500, end_ms: 22_000 },
  { phrase: 0, turn: 'asked', start_ms: 23_500, end_ms: 35_000 },
  { phrase: 1, turn: 'asked', start_ms: 36_500, end_ms: 48_000 },
]

describe('a lesson played through', () => {
  it('knows which stretch is playing', () => {
    expect(stretchAt(chapters, 0)).toBe(0)
    expect(stretchAt(chapters, 10_000)).toBe(0)
    expect(stretchAt(chapters, 10_500)).toBe(1)
    expect(stretchAt(chapters, 60_000)).toBe(3)
  })

  it('counts a phrase heard once it has been asked to the end, not when it was only heard', () => {
    expect(askedBy(chapters, 9_000)).toEqual([])
    expect(askedBy(chapters, 21_999)).toEqual([])
    expect(askedBy(chapters, 22_000)).toEqual([1])
    expect(askedBy(chapters, 35_000)).toEqual([0, 1])
  })

  it('skips to the next stretch, and has none after the last', () => {
    expect(nextStart(chapters, 3_000)).toBe(10_500)
    expect(nextStart(chapters, 40_000)).toBeNull()
  })

  it('goes back to the start of the stretch, or to the one before in its first two seconds', () => {
    expect(previousStart(chapters, 15_000)).toBe(10_500)
    expect(previousStart(chapters, 11_000)).toBe(0)
    expect(previousStart(chapters, 1_000)).toBe(0)
    expect(previousStart([], 5_000)).toBe(0)
  })
})

describe('what the device remembers', () => {
  const fresh: Listened = { at: 0, heard: {}, graded: false }

  it('keeps the moment each phrase was first heard', () => {
    const once = hear(fresh, chapters, 22_000, 1_000)
    expect(once.heard).toEqual({ 1: 1_000 })
    const twice = hear(once, chapters, 48_000, 5_000)
    // Phrase 1 was heard at 1 000 and is not heard again later.
    expect(twice.heard).toEqual({ 0: 5_000, 1: 1_000 })
    expect(twice.at).toBe(48_000)
  })

  it('leaves the state alone when nothing moved, or less than a second did', () => {
    const state = hear(fresh, chapters, 22_000, 1_000)
    expect(hear(state, chapters, 22_000, 9_000)).toBe(state)
    expect(hear(state, chapters, 22_900, 9_000)).toBe(state)
    expect(hear(state, chapters, 23_000, 9_000).at).toBe(23_000)
    // Back is a move too.
    expect(hear(state, chapters, 10_000, 9_000).at).toBe(10_000)
  })
})

describe('the words of a player', () => {
  it('says a length in minutes, never none', () => {
    expect(minutesLine(24 * 60_000 + 20_000)).toBe('24 min')
    expect(minutesLine(10_000)).toBe('1 min')
  })

  it('shows a moment as minutes and seconds', () => {
    expect(clock(0)).toBe('0:00')
    expect(clock(725_400)).toBe('12:05')
  })
})
