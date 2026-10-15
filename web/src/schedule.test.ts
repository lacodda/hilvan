import { describe, expect, it } from 'vitest'

import type { Slip } from '@/api'
import { awayLine, breakLine, dayName, noteLine, roomLine, slipDetail, slipNow, startOf } from '@/schedule'

describe('days', () => {
  it('names a day the way a person says it', () => {
    expect(dayName(new Date(2026, 8, 11, 20, 30))).toBe('Friday 11 September')
  })

  it('starts the day back at its first moment, on this device', () => {
    const morning = new Date(startOf('2026-11-01'))
    expect([morning.getFullYear(), morning.getMonth(), morning.getDate(), morning.getHours(), morning.getMinutes()]).toEqual([2026, 10, 1, 0, 0])
  })
})

describe('a break', () => {
  const since = new Date(2026, 9, 15, 19).toISOString()

  it('says until when, when there is a day back', () => {
    expect(breakLine({ id: 1, starts_at: since, ends_at: new Date(2026, 10, 1).toISOString() })).toBe(
      'On a break since Thursday 15 October. Back on Sunday 1 November - nothing comes due until then.',
    )
  })

  it('waits for the learner when there is not', () => {
    expect(breakLine({ id: 1, starts_at: since, ends_at: null })).toBe(
      'On a break since Thursday 15 October. Nothing comes due until you are back.',
    )
  })

  it('offers an absence as one, from the last answer', () => {
    expect(awayLine(new Date(2026, 8, 11, 21).toISOString())).toMatch(/^You have been away since Friday 11 September\. Count it as a break\?/)
  })
})

describe('the day that began owing', () => {
  it('says nothing when nothing is owed', () => {
    expect(roomLine({ debt: 0, words: 10, formulas: 1 })).toBeNull()
  })

  it('says how many new words the debt left', () => {
    expect(roomLine({ debt: 3, words: 7, formulas: 1 })).toBe('3 cards were left over from earlier days, so 7 new words open today instead of 10.')
    expect(roomLine({ debt: 1, words: 9, formulas: 1 })).toBe('1 card was left over from earlier days, so 9 new words open today instead of 10.')
    expect(roomLine({ debt: 9, words: 1, formulas: 1 })).toBe('9 cards were left over from earlier days, so 1 new word opens today instead of 10.')
  })

  it('says when nothing new opens at all', () => {
    expect(roomLine({ debt: 14, words: 0, formulas: 0 })).toBe('14 cards were left over from earlier days, so nothing new opens today.')
  })
})

describe('what slipped', () => {
  it('names the first few and counts the rest', () => {
    expect(noteLine({ starts_on: '2026-10-05', slipped: 1, names: ['to be'] })).toBe('1 slipped: "to be". It comes back sooner now.')
    expect(noteLine({ starts_on: '2026-10-05', slipped: 3, names: ['to be', 'doctor', 'home'] })).toBe(
      '3 slipped: "to be", "doctor" and "home". They come back sooner now.',
    )
    expect(noteLine({ starts_on: '2026-10-05', slipped: 7, names: ['to be', 'doctor', 'home'] })).toBe(
      '7 slipped: "to be", "doctor", "home" and 4 more. They come back sooner now.',
    )
  })

  const slip = (more: Partial<Slip>): Slip => ({
    kind: 'formula',
    id: 'f',
    name: 'to be',
    direction: 'produce',
    gloss: null,
    at: '',
    stitch: 'basted',
    declared: false,
    ...more,
  })

  it('says what slipped of it, and where it stands now', () => {
    expect(slipDetail(slip({}))).toBe('saying it')
    expect(slipDetail(slip({ direction: 'recognise' }))).toBe('understanding it')
    expect(slipDetail(slip({ kind: 'word', direction: null, gloss: 'врач', declared: true }))).toBe('врач - you had said you knew it')
    expect(slipNow(slip({ stitch: 'sewn' }))).toBe('sewn again')
  })
})
