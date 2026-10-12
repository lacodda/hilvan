import { describe, expect, it } from 'vitest'

import type { Snapshot, Standings } from '@/api'
import { change, dayOf, endOf, firstLook, lines, opening } from '@/then'

const snapshot = (sewn: number, words: number, answers: number): Snapshot => ({
  at: '2026-03-01T20:59:59.999Z',
  progress: {
    produce: { new: 30 - sewn - 2, basted: 2, sewn },
    recognise: { new: 30 - sewn, basted: 0, sewn },
  },
  words: {
    counts: { new: 0, basted: 0, sewn: 0 },
    levels: [
      { size: 1000, basted: 0, sewn: words },
      { size: 2000, basted: 0, sewn: words + 10 },
      { size: 5000, basted: 0, sewn: words + 12 },
    ],
    waiting: 0,
    declared: 0,
  },
  answers,
})

describe('days', () => {
  it('asks about the last moment of the day, on the learner own clock', () => {
    const end = new Date(endOf('2026-03-01'))
    expect(dayOf(end)).toBe('2026-03-01')
    expect(end.getHours()).toBe(23)
    expect(end.getMinutes()).toBe(59)
  })

  it('looks back a month first', () => {
    expect(firstLook(new Date(2026, 9, 12, 15, 0))).toBe('2026-09-12')
    expect(firstLook(new Date(2026, 2, 3, 9, 0))).toBe('2026-02-01')
  })
})

describe('opening', () => {
  const now = new Date(2026, 9, 12, 15, 0)
  const asker = (began: Date | null) => {
    const asked: string[] = []
    const ask = (at: string): Promise<Standings> => {
      asked.push(dayOf(at))
      return Promise.resolve({ then: { ...snapshot(0, 0, 0), at }, now: snapshot(1, 1, 1), began: began?.toISOString() ?? null })
    }
    return { asked, ask }
  }

  it('opens on a month ago for a learner who began before it', async () => {
    const { asked, ask } = asker(new Date(2026, 5, 1, 19, 0))
    expect(dayOf((await opening(ask, now)).then.at)).toBe('2026-09-12')
    expect(asked).toEqual(['2026-09-12'])
  })

  it('opens on the first day of a learner newer than a month', async () => {
    const { asked, ask } = asker(new Date(2026, 9, 1, 19, 0))
    expect(dayOf((await opening(ask, now)).then.at)).toBe('2026-10-01')
    expect(asked).toEqual(['2026-09-12', '2026-10-01'])
  })

  it('stays on a month ago for a learner who has not begun', async () => {
    const { ask } = asker(null)
    expect(dayOf((await opening(ask, now)).then.at)).toBe('2026-09-12')
  })
})

describe('lines', () => {
  it('compares formulas, every level and the answers', () => {
    const compared = lines(snapshot(3, 400, 20), snapshot(12, 650, 180))
    expect(compared.map((line) => line.label)).toEqual([
      'Formulas started',
      'Formulas you can say',
      'Formulas you understand',
      'Words of the 1k level',
      'Words of the 2k level',
      'Words of the 5k level',
      'Answers given',
    ])
    expect(compared[0]).toEqual({ label: 'Formulas started', then: 5, now: 14 })
    expect(compared.map(change)).toEqual(['+9', '+9', '+9', '+250', '+250', '+250', '+160'])
  })

  it('says nothing for no change and signs a fall', () => {
    expect(change({ label: 'x', then: 4, now: 4 })).toBe('')
    expect(change({ label: 'x', then: 4, now: 1 })).toBe('-3')
  })
})
