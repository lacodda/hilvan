import { readFileSync } from 'node:fs'

import { beforeAll, describe, expect, it } from 'vitest'

import type { Coverage, Hold, Line } from '@/api'
import { checkerOf, type Checker } from '@/core'
import type { ShelfItem } from '@/api'
import { chapterName, coverageLine, fit, levelLine, nextLine, nextReason, offsets, paragraphAt, placeIn, placeLine, restOf, runs, shelfOrder, swipe, SWIPE, throughText, tokensOf, withHolds, yours } from '@/reading'

describe('a swipe across a paragraph', () => {
  const start = { x: 40, y: 300 }

  it('says the rest known once it has gone far enough to the right', () => {
    expect(swipe(start, 40 + SWIPE - 1, 305)).toBe('watching')
    expect(swipe(start, 40 + SWIPE, 305)).toBe('known')
  })

  it('is a scroll the moment it runs down the page, however far it went sideways', () => {
    expect(swipe(start, 40 + SWIPE * 2, 340)).toBe('abandon')
  })

  it('is nothing to the left', () => {
    expect(swipe(start, 0, 300)).toBe('abandon')
  })

  it('reads the paragraph whose top passed the middle last', () => {
    expect(paragraphAt([-400, -10, 380, 900], 400)).toBe(2)
    expect(paragraphAt([500, 900], 400)).toBe(0)
    expect(paragraphAt([null, 100, null], 400)).toBe(1)
  })
})

const none: Coverage = { words: 0, sewn: 0, basted: 0, deck: 0, unknown: 0, beyond: 0, names: 0 }

describe('a sentence cut at its words', () => {
  it('keeps every character, the words as tokens and the rest between', () => {
    const line = { text: 'She went home.', tokens: [{ start: 0, length: 3, parts: [[0]], name: false }, { start: 4, length: 4, parts: [[1]], name: false }, { start: 9, length: 4, parts: [[2]], name: false }] }
    expect(runs(line)).toEqual([
      { text: 'She', token: 0 },
      { text: ' ', token: null },
      { text: 'went', token: 1 },
      { text: ' ', token: null },
      { text: 'home', token: 2 },
      { text: '.', token: null },
    ])
  })

  it('counts in code points, as the tutor does', () => {
    // An emoji is two UTF-16 units and one character: a word after it starts one character on.
    const line = { text: '😀 cat', tokens: [{ start: 2, length: 3, parts: [[0]], name: false }] }
    expect(runs(line)).toEqual([
      { text: '😀 ', token: null },
      { text: 'cat', token: 0 },
    ])
  })

  it('leaves a token that does not fit as plain text', () => {
    const line = { text: 'Hi.', tokens: [{ start: 1, length: 9, parts: [[0]], name: false }] }
    expect(runs(line)).toEqual([{ text: 'Hi.', token: null }])
  })
})

describe('the rest of a paragraph', () => {
  const paragraph: Pick<Line, 'tokens'>[] = [
    {
      tokens: [
        { start: 0, length: 3, parts: [[0]], name: false }, // she: sewn
        { start: 4, length: 4, parts: [[1, 2]], name: false }, // left: leave or left, neither held
        { start: 9, length: 5, parts: [[3], [4]], name: false }, // don't: do held, not unknown
        { start: 15, length: 5, parts: [], name: true }, // Harry
        { start: 21, length: 5, parts: [[5]], name: false }, // in the deck
      ],
    },
  ]
  const lemmas = ['en:she', 'en:leave', 'en:left', 'en:do', 'en:not', 'en:honey']
  const holds: (Hold | null)[] = ['sewn', null, null, 'sewn', null, 'deck']
  it('says known each part not held, as its commonest reading, and nothing in hand, in the deck or a name', () => {
    expect(restOf(paragraph, lemmas, holds)).toEqual(['en:leave', 'en:not'])
  })

  it('leaves a name alone even when the tutor read lemmas into it', () => {
    const named = [{ tokens: [{ start: 0, length: 4, parts: [[1]], name: true }] }]
    expect(restOf(named, lemmas, holds)).toEqual([])
  })

  it('finds where each line starts among the standings of the text', () => {
    expect(offsets([[{ tokens: [] }, { tokens: paragraph[0]!.tokens }], [{ tokens: paragraph[0]!.tokens }]])).toEqual([[0, 0], [5]])
  })
})

describe('what a change says', () => {
  it('replaces the holds of the words it names and keeps the rest', () => {
    expect(withHolds(['en:a', 'en:b', 'en:c'], ['sewn', null, 'deck'], { 'en:b': 'sewn', 'en:c': null })).toEqual(['sewn', 'sewn', null])
  })
})

describe('how much is yours', () => {
  it('is the held and the basted out of the counted words, rounded down', () => {
    expect(yours({ ...none, words: 3, sewn: 1, basted: 1, unknown: 1 })).toBe(66)
    expect(yours({ ...none, words: 200, sewn: 199, unknown: 1 })).toBe(99)
    expect(yours(none)).toBeNull()
    expect(coverageLine({ ...none, words: 4, sewn: 3, deck: 1 })).toBe('75% yours, 1 in the deck')
    expect(coverageLine(none)).toBe('No words to count')
  })

  it('says where the learner is', () => {
    const chapter = { position: 0, title: null, paragraph: 0, paragraphs: 12, coverage: none }
    const at = { paragraph: 2, paragraphs: 12, finished_at: null, opened_at: '2026-10-20T18:00:00Z', chapters: 1, chapter }
    expect(placeLine(at)).toBe('Paragraph 3 of 12')
    expect(placeLine({ ...at, opened_at: null })).toBe('Not opened yet')
    expect(placeLine({ ...at, finished_at: '2026-10-21T18:00:00Z' })).toBe('Read')
    const book = { ...at, paragraph: 40, paragraphs: 300, chapters: 12, chapter: { ...chapter, position: 2 } }
    expect(placeLine(book)).toBe('Chapter 3 of 12')
    expect(placeLine({ ...book, opened_at: null })).toBe('Not opened yet · 12 chapters')
  })

  it('keeps the place through the whole book, and finds it in the chapter it is in', () => {
    const chapter = (position: number, paragraph: number, paragraphs: number) => ({ position, title: null, paragraph, paragraphs, coverage: none })
    const contents = [chapter(0, 0, 4), chapter(1, 4, 10), chapter(2, 14, 3)]
    expect(placeIn({ paragraph: 6, contents, shown: 1 })).toBe(2)
    expect(placeIn({ paragraph: 4, contents, shown: 1 })).toBe(0)
    expect(placeIn({ paragraph: 14, contents, shown: 1 })).toBeNull()
    expect(placeIn({ paragraph: 3, contents, shown: 1 })).toBeNull()
    expect(throughText(contents[1]!, 2)).toBe(6)
  })

  it('names a chapter by its title, or its number where the book names none', () => {
    expect(chapterName({ position: 4, title: 'The Storm' })).toBe('The Storm')
    expect(chapterName({ position: 4, title: null })).toBe('Chapter 5')
  })
})

describe('the next text', () => {
  it('stands inside the window from 95 to 98 percent, or under or over it', () => {
    expect([fit(94), fit(95), fit(98), fit(99)]).toEqual(['harder', 'inside', 'inside', 'easier'])
  })

  it('says what it is and why', () => {
    const text = { kind: 'text', id: 3, title: 'Alice', chapter: 2, chapter_title: 'CHAPTER III.', yours: 96, started: true } as const
    expect(nextLine(text)).toEqual({ title: 'Alice', detail: 'CHAPTER III. · 96% yours' })
    expect(nextLine({ ...text, chapter: null, chapter_title: null })).toEqual({ title: 'Alice', detail: '96% yours' })
    const shelf = { kind: 'shelf', id: 'simple-cat', title: 'Cat', author: null, estimate: 88 } as const
    expect(nextLine(shelf)).toEqual({ title: 'Cat', detail: 'From the shelf · about 88% yours' })
    expect(nextReason(text)).toMatch(/^Between 95% and 98%/)
    expect(nextReason(shelf)).toMatch(/the nearest/)
  })

  it('puts the shelf in the order of how much is yours, the shorter first between equals', () => {
    const item = (id: string, estimate: number, words: number): ShelfItem => ({ id, kind: 'book', title: id, author: null, source: id, words, level: 2000, estimate, taken: null })
    expect(shelfOrder([item('a', 80, 10), item('b', 90, 500), item('c', 90, 100)]).map((one) => one.id)).toEqual(['c', 'b', 'a'])
    expect(levelLine(2000)).toBe('2,000 words')
    expect(levelLine(null)).toBe('more than 10,000 words')
  })
})

// The core counts the page: the same arithmetic as the server's list, run on
// the device after every tap.
let checker: Checker
beforeAll(async () => {
  checker = await checkerOf(readFileSync(new URL('./check.wasm', import.meta.url)))
})

describe('the page, counted by the core', () => {
  it('counts the words of a text as the tutor does', () => {
    const text = {
      lines: [[{ sentence: 1, text: "Harry didn't go.", tokens: [{ start: 0, length: 5, parts: [], name: true }, { start: 6, length: 6, parts: [[0], [1]], name: false }, { start: 13, length: 2, parts: [[2]], name: false }] }]],
    }
    const { coverage, standings } = checker.cover(tokensOf(text), ['sewn', null, 'basted'])
    expect(standings).toEqual(['name', 'unknown', 'basted'])
    expect(coverage).toEqual({ ...none, words: 2, basted: 1, unknown: 1, names: 1 })
  })
})
