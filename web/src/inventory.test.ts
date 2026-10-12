import { describe, expect, it } from 'vitest'

import type { InventoryItem } from '@/api'
import { currentLevel, keyAnswer, MOST_AT_ONCE, nextRequest, ordinal, reachLine, stillToAsk, withdraw, type Op } from '@/inventory'

const item = (rank: number, lemma = `w${rank}`): InventoryItem => ({ word: `en:${lemma}`, lemma, ipa: null, rank })
const answer = (word: string, known = true): Op => ({ kind: 'answer', answer: { word, known } })

describe('stillToAsk', () => {
  it('leaves out what was answered here and the server has not heard of yet', () => {
    const next = [item(1), item(2), item(3)]
    expect(stillToAsk(next, [], new Set(['en:w1'])).map((it) => it.rank)).toEqual([2, 3])
  })

  it('puts a word taken back where its rank puts it, once', () => {
    const next = [item(3), item(4)]
    const asked = stillToAsk(next, [item(1), item(4)], new Set())
    expect(asked.map((it) => it.rank)).toEqual([1, 3, 4])
  })
})

describe('nextRequest', () => {
  it('sends the answers at the head of the line together', () => {
    expect(nextRequest([answer('en:a'), answer('en:b', false), { kind: 'undo', word: 'en:a' }])).toEqual({
      ops: 2,
      answers: [
        { word: 'en:a', known: true },
        { word: 'en:b', known: false },
      ],
    })
  })

  it('sends a taking-back alone, after what came before it', () => {
    expect(nextRequest([{ kind: 'undo', word: 'en:a' }, answer('en:b')])).toEqual({ ops: 1, undo: 'en:a' })
    expect(nextRequest([])).toBeNull()
  })

  it('never sends more than a request carries', () => {
    const many = Array.from({ length: MOST_AT_ONCE + 7 }, (_, index) => answer(`en:w${index}`))
    expect(nextRequest(many)).toMatchObject({ ops: MOST_AT_ONCE })
  })
})

describe('withdraw', () => {
  it('takes back an answer that has not left yet', () => {
    expect(withdraw([answer('en:a'), answer('en:b')], 'en:b', 1)).toEqual([answer('en:a')])
  })

  it('leaves an answer on its way to the server to be taken back there', () => {
    expect(withdraw([answer('en:a'), answer('en:b')], 'en:a', 1)).toBeNull()
    expect(withdraw([], 'en:a', 0)).toBeNull()
  })
})

describe('levels', () => {
  it('works through the first level with words left', () => {
    const levels = [
      { size: 1000, left: 0, known: 700 },
      { size: 2000, left: 640, known: 900 },
      { size: 5000, left: 3640, known: 900 },
    ]
    expect(currentLevel(levels)?.size).toBe(2000)
    expect(currentLevel([{ size: 1000, left: 0, known: 1 }])).toBeNull()
    expect(reachLine({ size: 2000, left: 640, known: 900 })).toBe('1360 of the commonest 2000 gone through - 900 you know')
  })
})

describe('keyAnswer', () => {
  it('answers by the place of the key, whatever the layout', () => {
    expect(keyAnswer('ArrowRight')).toBe('know')
    expect(keyAnswer('KeyY')).toBe('know')
    expect(keyAnswer('KeyN')).toBe('unknown')
    expect(keyAnswer('Backspace')).toBe('undo')
    expect(keyAnswer('KeyQ')).toBeNull()
  })
})

describe('ordinal', () => {
  it('says a rank the way it is spoken', () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111, 1301].map(ordinal)).toEqual([
      '1st',
      '2nd',
      '3rd',
      '4th',
      '11th',
      '12th',
      '13th',
      '21st',
      '22nd',
      '101st',
      '111th',
      '1301st',
    ])
  })
})
