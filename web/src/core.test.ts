import { readFileSync } from 'node:fs'

import { beforeAll, describe, expect, it } from 'vitest'

import type { Formula, Value } from '@/api'
import { checkerOf, Unchecked, type Checker } from '@/core'
import { render } from '@/prompts'

// The real module, built from crates/check by `pnpm test`: what is under test
// is that the app reads the server's code the way the server does.
let checker: Checker
beforeAll(async () => {
  checker = await checkerOf(readFileSync(new URL('./check.wasm', import.meta.url)))
})

const formula: Formula = {
  id: 'be-present',
  name: 'to be',
  pattern: '<pronoun> + am/is/are + <rest>',
  say: '<pronoun> <pronoun:be> <rest>.',
  explanation: 'explained',
  samples: [{ sentence: 1, native: 'Я дома.', target: "I'm at home.", words: [{ word: 'en:home', lemma: 'home', form: 'home', start: 8 }], accept: ['I am at home.'] }],
  slots: [
    {
      name: 'pronoun',
      takes: null,
      values: [
        { native: 'я', target: 'I', forms: { be: 'am', short: "I'm" }, own: false },
        { native: 'он', target: 'he', forms: { be: 'is', short: "he's" }, own: false },
        { native: 'вы', target: 'you', forms: { be: 'are', short: "you're" }, own: false },
      ],
    },
    {
      name: 'rest',
      takes: 'noun',
      values: [
        { native: 'дома', target: 'at home', forms: {}, own: false },
        { native: 'в машине', target: 'in the car', forms: {}, own: true },
      ],
    },
  ],
  family: 'be-present',
  form: 'statement',
  languages: { native: 'ru', target: 'en' },
  sisters: [],
  accept: ['<pronoun:short> <rest>.'],
  parents: [],
}

describe('the checker built from the core', () => {
  it('reads a sample against its sentence and the spellings it accepts', () => {
    expect(checker.check(formula, { asked: { sample: 1 }, typed: 'i am at home' }).correct).toBe(true)
    const missed = checker.check(formula, { asked: { sample: 1 }, typed: "I'm at hom" })
    expect(missed.failure).toBe('word')
    expect(missed.missed).toEqual(['home'])
  })

  it('reads a substitution against every template the formula accepts', () => {
    const asked = { values: { pronoun: 'he', rest: 'in the car' } }
    const contracted = checker.check(formula, { asked, typed: "He's in the car" })
    expect(contracted.correct).toBe(true)
    expect(contracted.answer).toBe('He is in the car.')
    expect(checker.check(formula, { asked, typed: 'He are in the car' }).failure).toBe('formula')
    expect(checker.check(formula, { asked, typed: 'In the car he is' }).failure).toBe('order')
  })

  it('says it could not check a turn the formula does not ask, rather than bringing the page down', () => {
    expect(() => checker.check(formula, { asked: { sample: 9 }, typed: 'x' })).toThrow(Unchecked)
    expect(() => checker.check(formula, { asked: { values: { pronoun: 'thou', rest: 'at home' } }, typed: 'x' })).toThrow(/has no "thou"/)
  })

  it('grades a round as the server does', () => {
    expect(checker.rate([null, null], true)).toBe('easy')
    expect(checker.rate([null, 'word'], true)).toBe('good')
    expect(checker.rate([null, null, null, 'order'], false)).toBe('hard')
    expect(checker.rate(['formula', 'order', null, null], false)).toBe('again')
  })

  it('agrees with the sentence the app says for every substitution', () => {
    // The app renders `say` itself to show and speak the answer; the checker
    // renders it in Rust to read the answer against. One sentence, or the
    // learner hears one thing and is marked against another.
    for (const pronoun of formula.slots[0]!.values) {
      for (const rest of formula.slots[1]!.values) {
        const chosen = new Map<string, Value>([
          ['pronoun', pronoun],
          ['rest', rest],
        ])
        const said = render(formula.say!, chosen)
        const checked = checker.check(formula, { asked: { values: { pronoun: pronoun.target, rest: rest.target } }, typed: said ?? '' })
        expect(checked.answer).toBe(said)
        expect(checked.correct).toBe(true)
      }
    }
  })
})
