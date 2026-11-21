import { describe, expect, it } from 'vitest'

import type { Stretch } from '@/api'
import { clock, hear, minutesLine, nextStart, playedThrough, PLAYED_MS, previousStart, stretchAt, type Listened } from '@/lessons'

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

  it('counts a phrase heard once its asking is played to the end, not when it was only heard', () => {
    expect(playedThrough(chapters, 8_500, 9_000)).toEqual([])
    expect(playedThrough(chapters, 21_500, 21_999)).toEqual([])
    expect(playedThrough(chapters, 21_500, 22_000)).toEqual([1])
    // An end is crossed once.
    expect(playedThrough(chapters, 22_000, 22_250)).toEqual([])
    expect(playedThrough(chapters, 34_000, 35_000)).toEqual([0])
  })

  it('hears nothing in a jump: a phrase skipped was not answered', () => {
    expect(playedThrough(chapters, 0, 40_000)).toEqual([])
    expect(playedThrough(chapters, 21_000, 21_000 + PLAYED_MS + 1)).toEqual([])
    // Nor going back.
    expect(playedThrough(chapters, 40_000, 21_000)).toEqual([])
    // A slow report from a locked phone is still playing.
    expect(playedThrough(chapters, 18_000, 18_000 + PLAYED_MS)).toEqual([1])
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
    const once = hear(fresh, chapters, 21_800, 22_000, 1_000)
    expect(once.heard).toEqual({ 1: 1_000 })
    const asked = hear(once, chapters, 34_900, 35_000, 5_000)
    const twice = hear(asked, chapters, 47_900, 48_000, 7_000)
    // Phrase 1 was heard at 1 000 and is not heard again later.
    expect(twice.heard).toEqual({ 0: 5_000, 1: 1_000 })
    expect(twice.at).toBe(48_000)
  })

  it('leaves the state alone when nothing moved, or less than a second did', () => {
    const state = hear(fresh, chapters, 21_800, 22_000, 1_000)
    expect(hear(state, chapters, 22_000, 22_000, 9_000)).toBe(state)
    expect(hear(state, chapters, 22_000, 22_900, 9_000)).toBe(state)
    expect(hear(state, chapters, 22_900, 23_000, 9_000).at).toBe(23_000)
    // Back is a move too, and hears nothing.
    const back = hear(state, chapters, 23_000, 10_000, 9_000)
    expect(back.at).toBe(10_000)
    expect(back.heard).toEqual(state.heard)
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
