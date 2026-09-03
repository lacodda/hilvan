import { describe, expect, it } from 'vitest'

import type { Formula } from '@/api'
import { ask, formLabels, instruction, ladder, nextPrompt, paceLine, seconds, substitute, tabs, whenBack } from '@/prompts'

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
  family: 'be-present',
  form: 'statement',
  sisters: [
    { id: 'be-present-negation', form: 'negation', name: 'not', pattern: 'x', stitch: 'basted' },
    { id: 'be-present-question', form: 'question', name: 'am?', pattern: 'y', stitch: 'new' },
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

describe('ask', () => {
  const prompt = { native: 'Я дома.', target: 'I am at home.', source: 'sample' } as const

  it('leaves a producing prompt as it is', () => {
    expect(ask(prompt, 'produce')).toEqual(prompt)
  })

  it('turns the same sentence round to ask for its meaning', () => {
    // The two directions must be the same sentence: asking a different one
    // and calling it the reverse of this card is a lie about what was drilled.
    expect(ask(prompt, 'recognise')).toEqual({
      native: 'I am at home.',
      target: 'Я дома.',
      source: 'sample',
    })
  })

  it('is its own inverse', () => {
    expect(ask(ask(prompt, 'recognise'), 'recognise')).toEqual(prompt)
  })

  it('says what is being asked for', () => {
    expect(instruction('produce')).toContain('English')
    expect(instruction('recognise')).toContain('mean')
  })
})

describe('ladder', () => {
  it('runs the formula through every person', () => {
    expect(ladder(formula)).toEqual([
      { native: 'first', target: 'I + am/is/are + at home' },
      { native: 'second', target: 'you + am/is/are + at home' },
    ])
  })

  it('holds the other slots still, so only the person moves', () => {
    // A rung where the noun changes too teaches nothing about the person, so
    // the fixture gives the held slot more than one value it could drift to.
    const roomy = {
      ...formula,
      slots: [
        { name: 'pronoun', values: [{ native: 'first', target: 'I' }, { native: 'second', target: 'you' }] },
        { name: 'rest', values: [{ native: 'third', target: 'at home' }, { native: 'fourth', target: 'tired' }] },
      ],
    }
    const rests = new Set(ladder(roomy).map((rung) => rung.target.split(' + ')[2]))
    expect(rests).toEqual(new Set(['at home']))
  })

  it('offers no ladder for a formula with no person to run through', () => {
    const lets = { ...formula, pattern: "Let's + <verb>", slots: [{ name: 'verb', values: [{ native: 'идти', target: 'go' }] }] }
    expect(ladder(lets)).toEqual([])
  })

  it('offers no ladder when there is nothing to substitute', () => {
    expect(ladder({ ...formula, slots: [] })).toEqual([])
  })
})

describe('pace', () => {
  it('reads a duration at a glance', () => {
    expect(seconds(3800)).toBe('3.8s')
    expect(seconds(45_200)).toBe('45s')
  })

  it('says nothing when there is no pace yet', () => {
    expect(paceLine(null)).toBeNull()
  })

  it('remarks only on a difference worth remarking on', () => {
    // A screen that comments on every answer stops being read.
    expect(paceLine({ typical_ms: 4000, last_ms: 4200, answers: 5 })).toBe('4.2s, about your usual')
    expect(paceLine({ typical_ms: 4000, last_ms: 9000, answers: 5 })).toContain('slower')
    expect(paceLine({ typical_ms: 4000, last_ms: 1500, answers: 5 })).toContain('quicker')
  })
})

describe('tabs', () => {
  it('marks the formula on screen as the current one', () => {
    // The property the switch rests on: the tab marked current is the card
    // that will be graded. When these two came apart, the card had to
    // apologise for it in a sentence of its own.
    const current = tabs(formula).filter((tab) => tab.current)
    expect(current.map((tab) => tab.id)).toEqual([formula.id])
  })

  it('follows the formula it is given, not the one the queue offered', () => {
    // Switching to the question makes the question current - that is what
    // makes "what is on screen is what is graded" true.
    const question = {
      ...formula,
      id: 'be-present-question',
      form: 'question' as const,
      sisters: [
        { id: 'be-present', form: 'statement' as const, name: 'is', pattern: 'x', stitch: 'sewn' as const },
        { id: 'be-present-negation', form: 'negation' as const, name: 'not', pattern: 'y', stitch: 'basted' as const },
      ],
    }
    const current = tabs(question).find((tab) => tab.current)
    expect(current?.id).toBe('be-present-question')
    expect(current?.form).toBe('question')
  })

  it('orders the forms the way they are learnt', () => {
    expect(tabs(formula).map((tab) => tab.form)).toEqual(['statement', 'negation', 'question'])
  })

  it('shows where each other form stands, and nothing for the current one', () => {
    const byForm = Object.fromEntries(tabs(formula).map((tab) => [tab.form, tab.stitch]))
    expect(byForm.statement).toBeNull()
    expect(byForm.negation).toBe('basted')
    expect(byForm.question).toBe('new')
  })

  it('offers no switch for a formula that stands alone', () => {
    expect(tabs({ ...formula, family: null, form: null, sisters: [] })).toEqual([])
  })

  it('names the form rather than giving an example of it', () => {
    // "I am / I am not / Am I?" reads well on be-present and is wrong on
    // past-simple; a tab that lies about where it leads is worse than a
    // plain one.
    for (const label of Object.values(formLabels)) {
      expect(label).not.toMatch(/\bam\b|\bis\b|\bare\b/i)
    }
  })
})
