/**
 * The inventory as the screen holds it: the words still to ask, and the
 * answers on their way to the server, in order.
 *
 * The screen never waits for the server between two words - a thousand in
 * an evening is a thumb's rhythm, not a request's - so it keeps what it has
 * not heard back about, and every answer from the server is read through it.
 */

import type { InventoryAnswer, InventoryItem, Reach } from '@/api'

/** Something to tell the server, in the order it happened. */
export type Op = { kind: 'answer'; answer: InventoryAnswer } | { kind: 'undo'; word: string }

/** The most answers one request carries. */
export const MOST_AT_ONCE = 50

/**
 * The words still to ask: the server's next ones and those taken back here,
 * commonest first, without the ones answered here that the server may not
 * have heard of yet.
 */
export function stillToAsk(
  next: readonly InventoryItem[],
  returned: readonly InventoryItem[],
  answered: ReadonlySet<string>,
): InventoryItem[] {
  const byWord = new Map<string, InventoryItem>()
  for (const item of [...returned, ...next]) byWord.set(item.word, item)
  return [...byWord.values()].filter((item) => !answered.has(item.word)).sort((a, b) => a.rank - b.rank)
}

/**
 * The next request to make: the answers waiting at the head of the line go
 * together, a taking-back goes alone - it has to reach the server after the
 * answer it takes back.
 */
export function nextRequest(pending: readonly Op[]): { ops: number; answers: InventoryAnswer[] } | { ops: 1; undo: string } | null {
  const head = pending[0]
  if (!head) return null
  if (head.kind === 'undo') return { ops: 1, undo: head.word }
  const answers: InventoryAnswer[] = []
  for (const op of pending) {
    if (op.kind !== 'answer' || answers.length === MOST_AT_ONCE) break
    answers.push(op.answer)
  }
  return { ops: answers.length, answers }
}

/**
 * Takes an answer back before it has left: the line without it. `null` when
 * it is already on its way - the first `sending` ops are - or gone, and the
 * server has to be told instead.
 */
export function withdraw(pending: readonly Op[], word: string, sending: number): Op[] | null {
  const at = pending.findLastIndex((op) => op.kind === 'answer' && op.answer.word === word)
  if (at < sending) return null
  return [...pending.slice(0, at), ...pending.slice(at + 1)]
}

/** The level the run is going through: the first with words left; null once every level is gone through. */
export function currentLevel(levels: readonly Reach[]): Reach | null {
  return levels.find((level) => level.left > 0) ?? null
}

/** How far through a level the run is, in a line. */
export function reachLine(reach: Reach): string {
  const through = reach.size - reach.left
  return `${through} of the commonest ${reach.size} gone through - ${reach.known} you know`
}

/** A rank as it is said: 1st, 22nd, 113th. */
export function ordinal(n: number): string {
  const teen = n % 100 >= 11 && n % 100 <= 13
  const suffix = teen ? 'th' : ({ 1: 'st', 2: 'nd', 3: 'rd' } as Record<number, string>)[n % 10] ?? 'th'
  return `${n}${suffix}`
}

/**
 * What a key does on the inventory, by the key's place rather than its
 * letter, so the Russian layout answers as well as the English one.
 */
export function keyAnswer(code: string): 'know' | 'unknown' | 'undo' | null {
  switch (code) {
    case 'ArrowRight':
    case 'KeyY':
      return 'know'
    case 'ArrowLeft':
    case 'KeyN':
      return 'unknown'
    case 'Backspace':
      return 'undo'
    default:
      return null
  }
}
