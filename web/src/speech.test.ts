import { describe, expect, it } from 'vitest'

import type { Budget } from '@/api'
import { forecastLine, remainingLine } from '@/budget'
import { sides, tempoFor, urlOf } from '@/speech'

const formula = { languages: { native: 'ru', target: 'en' } }
const sample = { native: 'Я дома.', target: 'I am at home.', source: 'sample' } as const
const substitution = { native: 'я + am/is/are + дома', target: 'I am at home.', source: 'substitution' } as const

describe('tempoFor', () => {
  it('says new material slowly and the rest at speaking speed', () => {
    expect(tempoFor('new', false)).toBe('slow')
    expect(tempoFor('basted', true)).toBe('slow')
    expect(tempoFor('basted', false)).toBe('normal')
    expect(tempoFor('sewn', false)).toBe('normal')
  })
})

describe('sides', () => {
  it('asks in the learner language and answers in the one being learnt when producing', () => {
    const { question, answer } = sides(sample, 'produce', formula)
    expect(question).toEqual({ text: 'Я дома.', language: 'ru', speakable: true })
    expect(answer).toEqual({ text: 'I am at home.', language: 'en', speakable: true })
  })

  it('turns round when recognising', () => {
    const { question, answer } = sides(sample, 'recognise', formula)
    expect(question.language).toBe('en')
    expect(answer.language).toBe('ru')
  })

  it('never reads a scaffold aloud', () => {
    // "я + am/is/are + дома" is not a sentence, and the server would refuse it.
    const produce = sides(substitution, 'produce', formula)
    expect(produce.question.speakable).toBe(false)
    expect(produce.answer.speakable).toBe(true)
    expect(urlOf(produce.question, 'normal')).toBeNull()

    const recognise = sides(substitution, 'recognise', formula)
    expect(recognise.question.speakable).toBe(true)
    expect(recognise.answer.speakable).toBe(false)
  })

  it('addresses a sentence by language, text and tempo', () => {
    const url = urlOf(sides(sample, 'produce', formula).answer, 'slow')
    expect(url).toBe('/api/speech?language=en&text=I+am+at+home.&tempo=slow')
  })
})

describe('the budget', () => {
  const budget = (overrides: Partial<Budget>): Budget => ({
    remaining: 20_000,
    limit: 30_000,
    resets_at: '2026-10-01T12:00:00Z',
    per_day: 100,
    runs_out_at: '2027-04-19T12:00:00Z',
    lasts: true,
    ...overrides,
  })

  it('counts what is left', () => {
    expect(remainingLine(budget({}))).toBe('20,000 of 30,000 credits left')
  })

  it('says it lasts until the reset', () => {
    expect(forecastLine(budget({}))).toBe('At 100 a day they last until the reset on Oct 1.')
  })

  it('warns when it runs out first', () => {
    const line = forecastLine(budget({ lasts: false, per_day: 1000, runs_out_at: '2026-09-15T12:00:00Z' }))
    expect(line).toContain('run out around Sep 15')
    expect(line).toContain('before the reset on Oct 1')
  })

  it('does not forecast from nothing', () => {
    expect(forecastLine(budget({ per_day: 0, runs_out_at: null }))).toBe('Nothing spent lately - they last until the reset on Oct 1.')
  })
})
