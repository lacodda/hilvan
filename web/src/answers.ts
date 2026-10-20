/**
 * Answers, as the screens give them: into the line on this device, with the
 * identity they travel under and the moment they were given, then on to the
 * tutor (`delivery.ts`). The screens never post an answer themselves - there
 * is one road, and an answer given at home takes it as one given
 * underground does.
 */

import type { Rating, Reviewed, Turn } from '@/api'
import { countWaiting, delivered } from '@/delivery'
import { enqueue, mintId } from '@/outbox'
import { formulaCard, wordCard } from '@/sitting'

/**
 * How long a screen waits for the tutor's reply before it says the answer
 * is kept on the device and moves on. Long enough for a stand a room away
 * to answer; short enough that a hung connection on a train is not felt.
 */
export const REPLY_PATIENCE_MS = 4_000

/**
 * How long an answer took, from the moment its card was put in front of the
 * learner: measured when it is given, in the handler that gives it.
 */
export function tookSince(startedAt: number | null): number | null {
  return startedAt === null ? null : Date.now() - startedAt
}

/** What became of an answer: the tutor's reply, or kept here for later. */
export type Outcome = { kind: 'taken'; reviewed: Reviewed } | { kind: 'kept' }

async function give(path: string, card: string, answer: Record<string, unknown>): Promise<Outcome> {
  const answerId = mintId()
  const answeredAt = Date.now()
  await enqueue({
    answerId,
    path,
    body: { answer_id: answerId, answered_at: new Date(answeredAt).toISOString(), ...answer },
    card,
    answeredAt,
  })
  void countWaiting()
  const reviewed = await delivered(answerId, REPLY_PATIENCE_MS)
  return reviewed ? { kind: 'taken', reviewed } : { kind: 'kept' }
}

/** Saying a formula: the typed turns are the answer, and the tutor grades them. */
export function answerSaid(id: string, turns: Turn[], easy: boolean, durationMs: number | null): Promise<Outcome> {
  return give(`/formulas/${encodeURIComponent(id)}/review`, formulaCard(id, 'produce'), {
    direction: 'produce',
    turns,
    easy,
    duration_ms: durationMs,
  })
}

/** Understanding a formula: heard, and graded by the learner. */
export function answerHeard(id: string, rating: Rating, durationMs: number | null): Promise<Outcome> {
  return give(`/formulas/${encodeURIComponent(id)}/review`, formulaCard(id, 'recognise'), {
    direction: 'recognise',
    rating,
    duration_ms: durationMs,
  })
}

/** A word, recalled in its sentence and graded by the learner. */
export function answerWord(id: string, rating: Rating, durationMs: number | null): Promise<Outcome> {
  return give(`/words/${encodeURIComponent(id)}/review`, wordCard(id), { rating, duration_ms: durationMs })
}
