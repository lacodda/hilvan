import { describe, expect, it } from 'vitest'

import type { Side } from '@/api'
import { gap, swapLine } from '@/pairs'

const side = (lemma: string, typed: number): Side => ({ id: `en:${lemma}`, lemma, gloss: null, typed_for_the_other: typed })

describe('a pair', () => {
  it('cuts its sentence at the word, counted in characters', () => {
    expect(gap({ text: 'He told me.', start: 3, length: 4 })).toEqual({ before: 'He ', word: 'told', after: ' me.' })
    // A character outside the basic plane is one character, not two.
    expect(gap({ text: '😀 at home', start: 2, length: 2 })).toEqual({ before: '😀 ', word: 'at', after: ' home' })
  })

  it('says which word came for which, and how often', () => {
    expect(swapLine([side('at', 0), side('in', 2)])).toBe('You typed in where at stood twice.')
    expect(swapLine([side('say', 1), side('tell', 3)])).toBe('You typed say where tell stood once, and tell where say stood 3 times.')
    expect(swapLine([side('a', 0), side('b', 0)])).toBeNull()
  })
})
