import { describe, expect, it } from 'vitest'

import type { Formula } from '@/api'
import { ask, formLabels, gradeLine, instruction, ladder, nextPrompt, paceLine, render, seconds, substitute, tabs, tally, verdictLine, whenBack } from '@/prompts'

const formula: Formula = {
  id: 'be-present',
  name: 'to be',
  pattern: '<pronoun> + am/is/are + <rest>',
  say: '<pronoun> <pronoun:be> <rest>.',
  explanation: 'explained',
  samples: [
    { sentence: 1, native: 'prompt one', target: 'I am at home.', words: [], accept: ["I'm at home."] },
    { sentence: 2, native: 'prompt two', target: 'He is a doctor.', words: [], accept: [] },
  ],
  slots: [
    {
      name: 'pronoun',
      takes: null,
      values: [
        { native: 'first', target: 'I', forms: { be: 'am' }, own: false },
        { native: 'second', target: 'you', forms: { be: 'are' }, own: false },
      ],
    },
    { name: 'rest', takes: 'noun', values: [{ native: 'third', target: 'at home', forms: {}, own: false }] },
  ],
  family: 'be-present',
  form: 'statement',
  languages: { native: 'ru', target: 'en' },
  sisters: [
    { id: 'be-present-negation', form: 'negation', name: 'not', pattern: 'x', stitch: 'basted' },
    { id: 'be-present-question', form: 'question', name: 'am?', pattern: 'y', stitch: 'new' },
  ],
  accept: ['<pronoun:short> <rest>.'],
  parents: [],
}

/** Always the first value of every slot, so a test can name the answer. */
const first = () => 0
/** Always the last, which is where an off-by-one would show. */
const last = () => 0.999999

describe('substitute', () => {
  it('prompts with the scaffold and answers with the sentence', () => {
    expect(substitute(formula, first)).toEqual({
      native: 'first + am/is/are + third',
      target: 'I am at home.',
      source: 'substitution',
      asked: { values: { pronoun: 'I', rest: 'at home' } },
    })
  })

  it('can reach the last value of a slot', () => {
    // A picker that never returns the final value silently drills half a slot.
    expect(substitute(formula, last)?.target).toBe('You are at home.')
  })

  it('leaves the choice the formula teaches in the prompt, and makes it in the answer', () => {
    // "am/is/are" is not a hole: picking the right one is the formula, so
    // the prompt offers all three and the answer holds exactly one.
    expect(substitute(formula, first)?.native).toContain('am/is/are')
    expect(substitute(formula, first)?.target).not.toContain('/')
  })

  it('refuses a formula with no sentence to say', () => {
    expect(substitute({ ...formula, say: null }, first)).toBeNull()
  })

  it('refuses a formula with no slots rather than showing a raw pattern', () => {
    expect(substitute({ ...formula, slots: [] }, first)).toBeNull()
  })

  it('refuses a slot with no values', () => {
    expect(substitute({ ...formula, slots: [{ name: 'pronoun', takes: null, values: [] }] }, first)).toBeNull()
  })

  it('refuses a slot the pattern never mentions', () => {
    const wrong = { ...formula, slots: [{ name: 'verb', takes: null, values: [{ native: 'a', target: 'b', forms: {}, own: false }] }] }
    expect(substitute(wrong, first)).toBeNull()
  })
})

describe('render', () => {
  const he = { native: 'он', target: 'he', forms: { be: 'is', s: 's' }, own: false }
  const i = { native: 'я', target: 'I', forms: { be: 'am', s: '' }, own: false }
  const home = { native: 'дома', target: 'at home', forms: {}, own: false }

  // The same cases as src/say.rs: the server checks the client's sentence
  // against its own rendering before speaking it, so the two must agree.
  it('agrees the form with the value chosen', () => {
    expect(render('<pronoun> <pronoun:be> <rest>.', new Map([['pronoun', he], ['rest', home]]))).toBe('He is at home.')
    expect(render('<pronoun> <pronoun:be> <rest>.', new Map([['pronoun', i], ['rest', home]]))).toBe('I am at home.')
  })

  it('opens a question with a capital', () => {
    expect(render('<pronoun:be> <pronoun> <rest>?', new Map([['pronoun', he], ['rest', home]]))).toBe('Is he at home?')
  })

  it('takes a suffix and leaves no gap for an empty one', () => {
    expect(render('<pronoun> want<pronoun:s> to go.', new Map([['pronoun', he]]))).toBe('He wants to go.')
    expect(render('<pronoun> want<pronoun:s> to go.', new Map([['pronoun', i]]))).toBe('I want to go.')
  })

  it('renders nothing rather than half a sentence', () => {
    expect(render('<rest:be>', new Map([['rest', home]]))).toBeNull()
  })
})

describe('nextPrompt', () => {
  it('opens on a worked sample, so the shape is shown before it is asked for', () => {
    expect(nextPrompt(formula, 0, first)).toEqual({
      native: 'prompt one',
      target: 'I am at home.',
      source: 'sample',
      asked: { sample: 1 },
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
  const prompt = { native: 'Я дома.', target: 'I am at home.', source: 'sample', asked: { sample: 1 } } as const

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
      asked: { sample: 1 },
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
      { native: 'first', target: 'I am at home.' },
      { native: 'second', target: 'You are at home.' },
    ])
  })

  it('holds the other slots still, so only the person moves', () => {
    // A rung where the noun changes too teaches nothing about the person, so
    // the fixture gives the held slot more than one value it could drift to.
    const roomy = {
      ...formula,
      slots: [
        formula.slots[0]!,
        {
          name: 'rest',
          takes: null,
          values: [
            { native: 'third', target: 'at home', forms: {}, own: false },
            { native: 'fourth', target: 'tired', forms: {}, own: false },
          ],
        },
      ],
    }
    expect(ladder(roomy).every((rung) => rung.target.endsWith('at home.'))).toBe(true)
  })

  it('offers no ladder for a formula with no person to run through', () => {
    const lets = { ...formula, pattern: "Let's + <verb>", say: "Let's <verb>.", slots: [{ name: 'verb', takes: 'verb' as const, values: [{ native: 'идти', target: 'go', forms: {}, own: false }] }] }
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

describe('own words in a substitution', () => {
  const car = { native: 'машина', target: 'a car', forms: {}, own: true }
  const withOwn: Formula = {
    ...formula,
    slots: [formula.slots[0]!, { ...formula.slots[1]!, values: [...formula.slots[1]!.values, car] }],
  }

  it("draws from the learner's own words half the time", () => {
    // The pronoun takes no own words, so it draws once; the rest tosses a
    // coin first - below a half is your own - then draws.
    const draws = [0, 0.2, 0]
    const fromOwn = substitute(withOwn, () => draws.shift() ?? 0)
    expect(fromOwn?.target).toBe('I am a car.')
    expect(fromOwn?.asked).toEqual({ values: { pronoun: 'I', rest: 'a car' } })

    const fromPack = [0, 0.7, 0]
    expect(substitute(withOwn, () => fromPack.shift() ?? 0)?.target).toBe('I am at home.')
  })

  it('names a sample by its sentence and a substitution by its values', () => {
    expect(nextPrompt(withOwn, 3, first).asked).toEqual({ sample: 2 })
  })
})

describe('verdictLine', () => {
  it('says what went wrong in the three ways the drill tells apart', () => {
    expect(verdictLine({ correct: true, failure: null, missed: [] })).toBe('Right.')
    expect(verdictLine({ correct: false, failure: 'formula', missed: [] })).toBe('The shape slipped.')
    expect(verdictLine({ correct: false, failure: 'formula', missed: ['ready'] })).toBe('The shape slipped, and "ready" did not come.')
    expect(verdictLine({ correct: false, failure: 'order', missed: [] })).toBe('The right words, in the wrong order.')
    expect(verdictLine({ correct: false, failure: 'word', missed: ['busy'] })).toBe('The shape came - "busy" did not.')
    expect(verdictLine({ correct: false, failure: 'word', missed: ['a', 'coffee'] })).toBe('The shape came - "a" and "coffee" did not.')
  })
})

describe('tally and gradeLine', () => {
  it('counts the right turns before grading', () => {
    expect(tally([{ correct: true }])).toBe('Right.')
    expect(tally([{ correct: true }, { correct: true }])).toBe('All 2 right.')
    expect(tally([{ correct: true }, { correct: false }, { correct: true }])).toBe('2 of 3 right.')
  })

  it('says the grade and where the missed words went', () => {
    expect(gradeLine('hard', [])).toBe('Hard - it came, with a slip.')
    expect(gradeLine('good', ['en:busy'])).toBe('Good. The word "busy" goes back to your words.')
    expect(gradeLine('good', ['en:busy', 'en:tea'])).toBe('Good. The words "busy" and "tea" go back to your words.')
  })
})
