import { describe, expect, it } from 'vitest'

import type { Formula } from '@/api'
import { nextPrompt, substitute, whenBack } from '@/prompts'

const formula: Formula = {
  id: 'be-present',
  name: 'to be',
  pattern: '<pronoun> + am/is/are + <rest>',
  explanation: 'explained',
  samples: [
    { native: 'prompt one', target: 'I am at home.' },
    { native: 'prompt two', target: 'He is a doctor.' },
  ],
  slots: [
    {
      name: 'pronoun',
      values: [
        { native: 'first', target: 'I' },
        { native: 'second', target: 'you' },
      ],
    },
    { name: 'rest', values: [{ native: 'third', target: 'at home' }] },
  ],
}

/** Always the first value of every slot, so a test can name the answer. */
const first = () => 0
/** Always the last, which is where an off-by-one would show. */
const last = () => 0.999999

describe('substitute', () => {
  it('fills every hole in both languages', () => {
    expect(substitute(formula, first)).toEqual({
      native: 'first + am/is/are + third',
      target: 'I + am/is/are + at home',
      source: 'substitution',
    })
  })

  it('can reach the last value of a slot', () => {
    // A picker that never returns the final value silently drills half a slot.
    expect(substitute(formula, last)?.target).toContain('you')
  })

  it('leaves the choice the formula teaches in the prompt', () => {
    // "am/is/are" is not a hole: picking the right one is the formula.
    expect(substitute(formula, first)?.target).toContain('am/is/are')
  })

  it('refuses a formula with no slots rather than showing a raw pattern', () => {
    expect(substitute({ ...formula, slots: [] }, first)).toBeNull()
  })

  it('refuses a slot with no values', () => {
    expect(substitute({ ...formula, slots: [{ name: 'pronoun', values: [] }] }, first)).toBeNull()
  })

  it('refuses a slot the pattern never mentions', () => {
    const wrong = { ...formula, slots: [{ name: 'verb', values: [{ native: 'a', target: 'b' }] }] }
    expect(substitute(wrong, first)).toBeNull()
  })
})

describe('nextPrompt', () => {
  it('opens on a worked sample, so the shape is shown before it is asked for', () => {
    expect(nextPrompt(formula, 0, first)).toEqual({
      native: 'prompt one',
      target: 'I am at home.',
      source: 'sample',
    })
  })

  it('substitutes on the turns in between', () => {
    expect(nextPrompt(formula, 1, first).source).toBe('substitution')
    expect(nextPrompt(formula, 2, first).source).toBe('substitution')
  })

  it('comes back to a sample so the drill stays real language', () => {
    expect(nextPrompt(formula, 3, first).source).toBe('sample')
    expect(nextPrompt(formula, 3, first).target).toBe('He is a doctor.')
  })

  it('falls back to samples when a formula cannot be substituted into', () => {
    const bare = { ...formula, slots: [] }
    expect(nextPrompt(bare, 1, first).source).toBe('sample')
  })

  it('never shows an empty card', () => {
    const empty = { ...formula, slots: [], samples: [] }
    expect(nextPrompt(empty, 1, first).native).toBe(formula.pattern)
  })
})

describe('whenBack', () => {
  it('says the interval the way a person would', () => {
    expect(whenBack(0)).toBe('later today')
    expect(whenBack(1)).toBe('tomorrow')
    expect(whenBack(4)).toBe('in 4 days')
    expect(whenBack(30)).toBe('in a month')
    expect(whenBack(95)).toBe('in 3 months')
  })
})
