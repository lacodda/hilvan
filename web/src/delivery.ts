/**
 * Getting the answers on this device to the tutor, whenever it can be
 * reached.
 *
 * The other half of `outbox.ts`: it drains the line in order and says what
 * is still waiting. "Reachable" means the tutor answered, not that the
 * browser thinks it has a connection: the stand lives on a home network, and
 * a phone with four bars of mobile data is out of reach as far as it is
 * concerned. `navigator.onLine` is a hint about when to try again, never an
 * answer.
 */

import { useSyncExternalStore } from 'react'

import type { Reviewed } from '@/api'
import { forget, land, pending, unkept, type Sending } from '@/outbox'

/** What the screens show about the line. */
export interface DeliveryState {
  /** True once a request has reached the tutor and not since failed. */
  reachable: boolean
  /** Answers on this device the tutor does not have yet. */
  waiting: number
  /** How many of those live only in this page, and go when it closes. */
  unkept: number
  /**
   * True when the tutor turned an answer away because this device is not
   * signed in: the answers wait for a sign-in, not for the tutor.
   */
  signIn: boolean
  /** Answers the tutor refused since the page opened: gone for good. */
  refused: number
}

let state: DeliveryState = { reachable: true, waiting: 0, unkept: 0, signIn: false, refused: 0 }
const listeners = new Set<() => void>()

function publish(patch: Partial<DeliveryState>): void {
  const next = { ...state, ...patch }
  const same = (Object.keys(next) as (keyof DeliveryState)[]).every((key) => next[key] === state[key])
  if (same) return
  state = next
  for (const listener of listeners) listener()
}

/** The line as it stands, for a screen. */
export function useDelivery(): DeliveryState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    () => state,
    () => state,
  )
}

/** The line as it stands, outside React. */
export const deliveryState = (): DeliveryState => state

/** Records that the tutor answered a request, or did not. */
export function sawTutor(reachable: boolean): void {
  publish({ reachable })
}

/** Re-reads how many answers are waiting. */
export async function countWaiting(): Promise<void> {
  try {
    publish({ waiting: (await pending()).length, unkept: unkept() })
  } catch {
    // The count is for the screen; failing to read it changes nothing.
  }
}

/**
 * Answers that refuse the answer itself rather than the moment: a card no
 * pack has any more (404), an answer the tutor cannot read (400, 422).
 * Retrying them would block every answer behind them, so they are dropped
 * and counted. Nothing else is: dropping cannot be undone, so an answer not
 * on this list is kept rather than guessed about.
 */
const REFUSED = new Set([400, 404, 422])

/** Answers that refuse the moment: this device is not signed in. */
const SIGNED_OUT = new Set([401, 403])

/**
 * How long one delivery may take before the tutor counts as out of reach.
 * A connection on a train can hang rather than fail, and a hung request
 * would hold every answer behind it.
 */
const PATIENCE_MS = 15_000

class Stop extends Error {
  readonly why: 'later' | 'signed-out'

  constructor(why: 'later' | 'signed-out') {
    super(why)
    this.why = why
  }
}

/** The answers a screen is waiting on, and the tutor's replies to them. */
const awaited = new Set<string>()
const replies = new Map<string, Reviewed>()

async function deliver(sending: Sending): Promise<'landed' | 'refused'> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), PATIENCE_MS)
  let response: Response
  try {
    response = await fetch(`/api${sending.path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(sending.body),
      signal: controller.signal,
    })
  } catch {
    throw new Stop('later')
  } finally {
    clearTimeout(timer)
  }
  if (response.ok) {
    if (awaited.has(sending.answerId)) {
      try {
        replies.set(sending.answerId, (await response.json()) as Reviewed)
      } catch {
        // Taken, with a reply that did not read: the answer is still in.
      }
    }
    return 'landed'
  }
  if (SIGNED_OUT.has(response.status)) throw new Stop('signed-out')
  if (REFUSED.has(response.status)) return 'refused'
  // A 5xx is the tutor having a bad moment, and the rest are not ours to
  // interpret: both are worth waiting for.
  throw new Stop('later')
}

let draining: Promise<void> | null = null

/**
 * Delivers everything waiting, oldest first. Calls made while a drain runs
 * share it: three drains at once would deliver the same answer three times.
 */
export function drain(): Promise<void> {
  draining ??= run().finally(() => {
    draining = null
  })
  return draining
}

async function run(): Promise<void> {
  let line: Sending[]
  try {
    line = await pending()
  } catch {
    return
  }
  publish({ waiting: line.length, unkept: unkept() })
  if (line.length === 0) {
    publish({ signIn: false })
    return
  }
  for (const sending of line) {
    let outcome: 'landed' | 'refused'
    try {
      outcome = await deliver(sending)
    } catch (cause) {
      // Everything from here on stays: order is the point, and an answer
      // delivered over a stuck earlier one would land out of sequence. A
      // tutor that wants a sign-in is a tutor that answered.
      if (cause instanceof Stop && cause.why === 'signed-out') publish({ reachable: true, signIn: true })
      else publish({ reachable: false })
      await countWaiting()
      return
    }
    if (outcome === 'landed') await land(sending, Date.now())
    else {
      await forget(sending)
      publish({ refused: state.refused + 1 })
    }
    publish({ reachable: true, signIn: false })
  }
  await countWaiting()
}

/**
 * Delivers the line and says what the tutor made of one answer in it: its
 * reply, or `null` when the tutor could not be reached in `patienceMs` -
 * the answer stays in the line, and the screen says it is kept on the
 * device. Answers older than it go first, as always.
 */
export async function delivered(answerId: string, patienceMs: number): Promise<Reviewed | null> {
  if (!state.reachable && typeof navigator !== 'undefined' && !navigator.onLine) {
    // Underground and known to be: no point holding the screen for a try.
    void drain()
    return null
  }
  awaited.add(answerId)
  const through = async () => {
    await drain()
    // A drain already under way read the line before this answer joined it:
    // one more pass takes it.
    if (!replies.has(answerId) && state.reachable && !state.signIn) await drain()
    return true
  }
  const waited = await Promise.race([through(), new Promise<false>((resolve) => setTimeout(() => resolve(false), patienceMs))])
  const reply = waited ? (replies.get(answerId) ?? null) : null
  // A reply that comes after the screen moved on is nobody's to read.
  awaited.delete(answerId)
  replies.delete(answerId)
  return reply
}
