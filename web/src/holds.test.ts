import { describe, expect, it } from 'vitest'

import { band, bands, holdLine, position, scale, segments } from '@/holds'

describe('the scale a card holds on', () => {
  it('puts a stability in the band it falls in', () => {
    expect(band(0)).toBeNull()
    expect(band(0.3)?.name).toBe('hours')
    expect(band(1)?.name).toBe('days')
    expect(band(15)?.name).toBe('weeks')
    expect(band(90)?.name).toBe('months')
    expect(band(5000)?.name).toBe('a year and more')
  })

  it('is logarithmic: an hour to a day is as long as a day to a day', () => {
    expect(position(1) - position(1 / 24)).toBeCloseTo(Math.log(24))
    // Past either end, a card stands at the end rather than off the bar.
    expect(position(0.0001)).toBe(scale.from)
    expect(position(100_000)).toBe(scale.to)
  })

  it('says how firmly it holds in words, never a number', () => {
    expect(holdLine(0)).toBe('Not held yet')
    expect(holdLine(12)).toBe('Holds for weeks')
    expect(holdLine(40, 12)).toBe('Holds for months now - it held for weeks')
    expect(holdLine(13, 12)).toBe('Holds for weeks')
    for (const days of [0.2, 3, 12, 40, 400]) {
      expect(holdLine(days, days / 3)).not.toMatch(/\d/)
    }
  })
})

describe('the bar', () => {
  const covered = (pieces: ReturnType<typeof segments>) => pieces.map((piece) => [piece.start, piece.end])

  it('runs the whole scale without gaps or overlaps', () => {
    for (const [days, was] of [[0, undefined], [12, undefined], [40, 12], [2, 40], [0.5, 0]] as const) {
      const pieces = covered(segments(days, was))
      expect(pieces[0]![0]).toBe(scale.from)
      expect(pieces.at(-1)![1]).toBe(scale.to)
      for (let index = 1; index < pieces.length; index += 1) {
        expect(pieces[index]![0]).toBe(pieces[index - 1]![1])
      }
    }
  })

  it('cuts at every band, so the bands show', () => {
    const edges = segments(12).map((piece) => piece.end)
    for (const band of bands) expect(edges).toContain(position(band.to))
  })

  it('fills to where the card stands, and no further', () => {
    const held = position(12)
    for (const piece of segments(12)) {
      expect(piece.tone).toBe(piece.end <= held ? 'accent' : 'idle')
    }
    expect(segments(0).every((piece) => piece.tone === 'idle')).toBe(true)
  })

  it('shows what an answer did: the growth bright, a fall as a warning', () => {
    const grew = segments(40, 12)
    expect(grew.filter((piece) => piece.end <= position(12)).every((piece) => piece.tone === 'past')).toBe(true)
    expect(grew.filter((piece) => piece.start >= position(12) && piece.end <= position(40)).every((piece) => piece.tone === 'accent')).toBe(true)

    const fell = segments(2, 40)
    expect(fell.filter((piece) => piece.start >= position(2) && piece.end <= position(40)).every((piece) => piece.tone === 'warn')).toBe(true)
    expect(fell.some((piece) => piece.tone === 'accent')).toBe(false)
  })
})
