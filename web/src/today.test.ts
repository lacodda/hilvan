import { describe, expect, it } from 'vitest'

import type { Due, Formula, Room, Word } from '@/api'
import { asOfLine, describeQueue, nextLine, waitingLine } from '@/today'

const formula = { id: 'f' } as Formula
const word = { id: 'en:doctor' } as Word
const common = { stitch: 'new', due: '', pace: null, stability: 0 } as const
const room: Room = { debt: 0, words: 10, formulas: 1 }

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

describe('nextLine', () => {
  it('says which formula is next and which parents hold it', () => {
    expect(nextLine(null, room)).toBeNull()
    expect(nextLine({ id: 'want', name: 'want to', waits_for: [] }, room)).toBe('Next: "want to", on the next day with room for a new formula.')
    expect(nextLine({ id: 'like', name: 'would like', waits_for: [{ id: 'want', name: 'want to', stitch: 'basted' }] }, room)).toBe(
      'Next: "would like", once "want to" is sewn.',
    )
    const two = [
      { id: 'a', name: 'can', stitch: 'new' as const },
      { id: 'b', name: 'want to', stitch: 'basted' as const },
    ]
    expect(nextLine({ id: 'like', name: 'would like', waits_for: two }, room)).toBe('Next: "would like", once "can" and "want to" are sewn.')
  })

  it('says when it is the debt that holds the formula of the day', () => {
    const owing: Room = { debt: 12, words: 0, formulas: 0 }
    expect(nextLine({ id: 'want', name: 'want to', waits_for: [] }, owing)).toBe('Next: "want to", on a day that begins with fewer cards left over.')
  })
})

describe('what the answers on this device wait for', () => {
  const quiet = { waiting: 0, unkept: 0, signIn: false, refused: 0 }

  it('says nothing when every answer is in', () => {
    expect(waitingLine(quiet)).toBeNull()
  })

  it('says whether they wait for the tutor or for a sign-in', () => {
    expect(waitingLine({ ...quiet, waiting: 1 })).toBe('1 answer on this device waits to reach the tutor.')
    expect(waitingLine({ ...quiet, waiting: 3, signIn: true })).toBe('3 answers on this device wait for you to sign in again.')
  })

  it('warns when the browser will not keep them, and counts what the tutor refused', () => {
    const line = waitingLine({ waiting: 2, unkept: 2, signIn: false, refused: 1 })
    expect(line).toContain('lost if the page is closed')
    expect(line).toContain('refused one answer')
  })
})

describe('a Today kept for the road', () => {
  it('says how old it is', () => {
    const now = new Date('2026-10-20T18:00:00')
    expect(asOfLine(new Date('2026-10-20T08:30:00').toISOString(), now)).toMatch(/^Out of the tutor's reach - this is today as it stood at 08:30\./)
    expect(asOfLine(new Date('2026-10-18T08:30:00').toISOString(), now)).toContain('on Sunday, October 18, 08:30')
  })
})
