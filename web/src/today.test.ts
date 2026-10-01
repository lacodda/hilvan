import { describe, expect, it } from 'vitest'

import type { Due, Formula, Word } from '@/api'
import { describeQueue } from '@/today'

const formula = { id: 'f' } as Formula
const word = { id: 'en:doctor' } as Word
const common = { stitch: 'new', due: '', pace: null } as const

const produce = (is_new: boolean): Due => ({ ...common, kind: 'formula', formula, direction: 'produce', is_new })
const recognise = (is_new: boolean): Due => ({ ...common, kind: 'formula', formula, direction: 'recognise', is_new })
const aWord = (is_new: boolean): Due => ({ ...common, kind: 'word', word, is_new })

describe('describeQueue', () => {
  it('names the one new formula of a first day', () => {
    expect(describeQueue([produce(true)])).toBe('One new formula to start.')
  })

  it('does not call the backwards side of a formula just met a new formula', () => {
    // The screen used to say "One new formula to start. 1 of them asked
    // backwards." straight after the first formula: the recognising card was
    // counted as new, because it is new in that direction.
    expect(describeQueue([recognise(true)])).toBe('1 formula to see backwards.')
  })

  it('tells the three kinds of formula turn apart', () => {
    expect(describeQueue([produce(false), produce(false), recognise(false), produce(true)])).toBe(
      '2 formulas to come back to, 1 to see backwards, then one new one.',
    )
  })

  it('puts the words after the formulas', () => {
    expect(describeQueue([produce(false), aWord(false), aWord(true), aWord(true)])).toBe(
      '1 formula to come back to. Then 1 word to come back to and 2 new.',
    )
  })

  it('says where new words come from when they are all there is', () => {
    expect(describeQueue([aWord(true), aWord(true)])).toBe('2 new words from the sentences you have met.')
    expect(describeQueue([aWord(false)])).toBe('1 word to come back to.')
  })
})
