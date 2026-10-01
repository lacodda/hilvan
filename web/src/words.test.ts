import { describe, expect, it } from 'vitest'

import { commonness, levelLine, levelName, mark } from '@/words'

describe('mark', () => {
  it('cuts a sentence around the word where the server says it starts', () => {
    expect(mark('She went to the doctor.', 4, 'went')).toEqual({ before: 'She ', word: 'went', after: ' to the doctor.' })
  })

  it('counts characters as the server does, not UTF-16 units', () => {
    // An emoji is one character to the server and two units to JavaScript;
    // counting units would mark "ent " instead of "went".
    expect(mark('🙂 She went home.', 6, 'went').word).toBe('went')
  })

  it('finds the word by its spelling when the position is off', () => {
    expect(mark('He is a Doctor.', 0, 'doctor')).toEqual({ before: 'He is a ', word: 'Doctor', after: '.' })
  })

  it('marks nothing rather than the wrong place when the word is not there', () => {
    expect(mark('He is here.', 3, 'doctor')).toEqual({ before: 'He is here.', word: '', after: '' })
  })
})

describe('levels', () => {
  it('names a level the way a learner says it', () => {
    expect(levelName(1000)).toBe('1k')
    expect(levelName(5000)).toBe('5k')
  })

  it('says what is held, and leaves out an empty basted count', () => {
    expect(levelLine({ size: 1000, sewn: 12, basted: 30 })).toBe('12 sewn, 30 basted of 1000')
    expect(levelLine({ size: 2000, sewn: 0, basted: 0 })).toBe('0 sewn of 2000')
  })

  it('tells how common a word is by its level', () => {
    expect(commonness({ level: 1000, rank: 910 })).toBe('among the 1000 commonest words')
    expect(commonness({ level: null, rank: 5419 })).toBe('rarer than any level')
    expect(commonness({ level: null, rank: null })).toBeNull()
  })
})
