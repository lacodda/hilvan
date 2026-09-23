/** The ElevenLabs budget and the engines, said the way a person would. */

import type { Budget, EngineState } from '@/api'

const day = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric' })
const count = new Intl.NumberFormat('en')

/** "20,000 of 30,000 credits left". */
export function remainingLine(budget: Budget): string {
  return `${count.format(budget.remaining)} of ${count.format(budget.limit)} credits left`
}

/**
 * Whether the credits last, and until when.
 *
 * The one question worth asking before a lesson: will this period's credits
 * hold until they start over? Said as a date, because "34.2 days" is a sum
 * the learner would have to do.
 */
export function forecastLine(budget: Budget): string {
  const reset = budget.resets_at ? day.format(new Date(budget.resets_at)) : null
  if (budget.runs_out_at === null) {
    return reset ? `Nothing spent lately - they last until the reset on ${reset}.` : 'Nothing spent lately.'
  }
  const out = day.format(new Date(budget.runs_out_at))
  if (budget.lasts) {
    return reset ? `At ${Math.round(budget.per_day)} a day they last until the reset on ${reset}.` : `At ${Math.round(budget.per_day)} a day they last until ${out}.`
  }
  return reset
    ? `At ${Math.round(budget.per_day)} a day they run out around ${out}, before the reset on ${reset}.`
    : `At ${Math.round(budget.per_day)} a day they run out around ${out}.`
}

/** What an engine's state means for the learner. */
export function engineLine(name: string, state: EngineState): string {
  switch (state) {
    case 'ok':
      return `${name} is speaking.`
    case 'unreachable':
      return `${name} is not answering - its sentences are silent until it is back.`
    case 'off':
      return `${name} is not set up.`
  }
}

/** The learner's word for a language code. */
export function languageName(code: string): string {
  try {
    return new Intl.DisplayNames(['en'], { type: 'language' }).of(code) ?? code
  } catch {
    return code
  }
}
