import { describe, expect, it } from 'vitest'

import type { Due, Formula, FormulaDue, Usual, WordDue } from '@/api'
import { badgeCount, cardOf, expected, fitting, minutesOf, plan, soundsOf, stillAsks, TURNS, unanswered } from '@/sitting'

const formula: Formula = {
  id: 'be',
  name: 'to be',
  pattern: '<pronoun> + am/is/are + <rest>',
  say: '<pronoun> <pronoun:be> <rest>.',
  explanation: 'Объяснение.',
  samples: [
    { sentence: 1, native: 'Я дома.', target: 'I am at home.', words: [], accept: [] },
    { sentence: 2, native: 'Он врач.', target: 'He is a doctor.', words: [], accept: [] },
  ],
  slots: [
    { name: 'pronoun', takes: null, values: [{ native: 'я', target: 'I', forms: { be: 'am' }, own: false }] },
    { name: 'rest', takes: null, values: [{ native: 'дома', target: 'at home', forms: {}, own: false }] },
  ],
  family: null,
  form: null,
  languages: { native: 'ru', target: 'en' },
  sisters: [],
  accept: [],
  parents: [],
}

function said(id: string, typical: number | null, isNew = false): FormulaDue {
  return {
    kind: 'formula',
    formula: { ...formula, id },
    direction: 'produce',
    stitch: isNew ? 'new' : 'basted',
    is_new: isNew,
    due: '2026-10-20T08:00:00Z',
    pace: typical === null ? null : { typical_ms: typical, last_ms: typical, answers: 4 },
    stability: 3,
  }
}

function word(id: string, isNew = false): WordDue {
  return {
    kind: 'word',
    word: {
      id,
      lemma: id.slice(3),
      gloss: 'x',
      ipa: null,
      rank: 1,
      level: 1000,
      contexts: [{ sentence: 1, text: 'I am at home.', translation: 'Я дома.', form: 'home', start: 8, formula: 'be', anchor: true }],
      languages: { native: 'ru', target: 'en' },
    },
    stitch: isNew ? 'new' : 'sewn',
    is_new: isNew,
    due: '2026-10-20T08:00:00Z',
    pace: null,
    stability: 20,
  }
}

const usual: Usual = { produce: 60_000, recognise: null, word: 10_000 }

describe('the queue less what was answered here', () => {
  it('takes off the cards answered on this device, by card and direction', () => {
    const queue: Due[] = [said('a', null), { ...said('a', null), direction: 'recognise' }, word('en:home')]
    const left = unanswered(queue, new Set(['formula:a:produce', 'word:en:home']))
    expect(left.map(cardOf)).toEqual(['formula:a:recognise'])
  })
})

describe('a sitting of a few minutes', () => {
  it('measures a card by its own pace, then by its kind, then by a first guess', () => {
    expect(expected(said('a', 30_000), usual)).toBe(30_000)
    expect(expected(said('a', null), usual)).toBe(60_000)
    expect(expected({ ...said('a', null), direction: 'recognise' }, usual)).toBe(40_000)
    expect(expected(word('en:home'), usual)).toBe(10_000)
  })

  it('takes from the front what fits, and skips nothing to fit more', () => {
    // Two minutes: a minute and a half, then a card that would run over -
    // and the quick word behind it is not taken in its place.
    const queue: Due[] = [said('a', 90_000), said('b', 120_000), word('en:home')]
    expect(fitting(queue, 2, usual).map(cardOf)).toEqual(['formula:a:produce'])
    expect(fitting(queue, 4, usual).map(cardOf)).toEqual(['formula:a:produce', 'formula:b:produce', 'word:en:home'])
  })

  it('always gives at least one card', () => {
    expect(fitting([said('slow', 20 * 60_000)], 5, usual)).toHaveLength(1)
    expect(fitting([], 5, usual)).toEqual([])
  })

  it('says about how long the whole queue takes, rounded up', () => {
    expect(minutesOf([said('a', 90_000), word('en:home')], usual)).toBe(2)
  })
})

describe('the number on the icon', () => {
  it('counts the reviews waiting and what has come back since, never the new', () => {
    const now = Date.parse('2026-10-20T12:00:00Z')
    const queue: Due[] = [said('a', null), said('b', null, true), word('en:home'), word('en:new', true)]
    const coming = ['2026-10-20T11:00:00Z', '2026-10-20T13:00:00Z']
    expect(badgeCount(queue, coming, now)).toBe(3)
  })
})

describe('the questions planned for a sitting', () => {
  const draws = () => 0

  it('draws each formula its turns once, and keeps them while it is in the queue', () => {
    const queue: Due[] = [said('a', null), word('en:home')]
    const first = plan(queue, {}, draws)
    expect(Object.keys(first)).toEqual(['formula:a:produce'])
    expect(first['formula:a:produce']).toHaveLength(TURNS)
    // Kept: drawn again with other dice, the plan is what it was.
    expect(plan(queue, first, () => 0.99)).toEqual(first)
  })

  it('forgets a card that has left the queue, and redraws a question the pack no longer asks', () => {
    const queue: Due[] = [said('a', null)]
    const kept = plan(queue, {}, draws)
    expect(plan([said('b', null)], kept, draws)['formula:a:produce']).toBeUndefined()

    const stale = kept['formula:a:produce']!.map((prompt) => ('sample' in prompt.asked ? { ...prompt, asked: { sample: 99 } } : prompt))
    const redrawn = plan(queue, { 'formula:a:produce': stale }, draws)['formula:a:produce']!
    expect(redrawn.every((prompt) => stillAsks(formula, prompt))).toBe(true)
  })

  it('knows a question the formula still asks from one it does not', () => {
    const sample = { native: 'Я дома.', target: 'I am at home.', source: 'sample' as const, asked: { sample: 1 } }
    expect(stillAsks(formula, sample)).toBe(true)
    expect(stillAsks(formula, { ...sample, target: 'I was at home.' })).toBe(false)
    const values = { native: 'я + am/is/are + дома', target: 'I am at home.', source: 'substitution' as const, asked: { values: { pronoun: 'I', rest: 'at home' } } }
    expect(stillAsks(formula, values)).toBe(true)
    expect(stillAsks(formula, { ...values, asked: { values: { pronoun: 'we', rest: 'at home' } } })).toBe(false)
  })
})

describe('the sounds a sitting plays', () => {
  it('holds every question and answer at the card speed, and a new formula its explanation', () => {
    const queue: Due[] = [said('a', null, true), word('en:home')]
    const planned = plan(queue, {}, () => 0)
    const urls = soundsOf(queue, planned).map((url) => Object.fromEntries(new URL(url, 'http://x').searchParams))
    expect(urls).toContainEqual({ language: 'ru', text: 'Объяснение.', tempo: 'normal' })
    // A new formula is said slowly; a sample question is said in Russian too.
    expect(urls).toContainEqual({ language: 'en', text: 'I am at home.', tempo: 'slow' })
    expect(urls).toContainEqual({ language: 'ru', text: 'Я дома.', tempo: 'slow' })
    // A word: its sentence, and the word slowly on its own.
    expect(urls).toContainEqual({ language: 'en', text: 'I am at home.', tempo: 'normal' })
    expect(urls).toContainEqual({ language: 'en', text: 'home', tempo: 'slow' })
    // Each once.
    expect(new Set(urls.map((url) => JSON.stringify(url))).size).toBe(urls.length)
  })
})
