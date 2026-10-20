/**
 * The checker: hilvan's core, built to WebAssembly from `crates/check`.
 *
 * A typed turn is checked here, on the device, the moment it is typed - with
 * the tutor in reach or not - by the same Rust the server grades the answer
 * with when it arrives. The server checks every turn again then, and that
 * reading is the one the grade rests on; this one says "right" or "the shape
 * slipped" on a train, and says it the way the server will.
 *
 * The module is four exports and two buffers (`crates/check/src/lib.rs`):
 * the request is written into its memory as JSON, the reply read back out.
 */

import type { Checked, Failure, Formula, Rating, Turn } from '@/api'
import wasmUrl from '@/check.wasm?url'

interface Exports {
  memory: WebAssembly.Memory
  input: (length: number) => number
  check: () => number
  rate: () => number
  output: () => number
}

/** What the checker answers, and everything a request can go wrong with. */
type Reply = { checked: Checked } | { rating: Rating } | { error: string }

/** The checker, ready to be called. */
export interface Checker {
  /** One typed turn, read against the formula that asked it. */
  check: (formula: Formula, turn: Turn) => Checked
  /** The grade a round of turns earns. */
  rate: (failures: readonly (Failure | null)[], easy: boolean) => Rating
}

/** A turn the checker could not read: a formula and a turn out of step. */
export class Unchecked extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'Unchecked'
  }
}

/** The checker over a module's bytes: what the app and the tests both build on. */
export async function checkerOf(source: BufferSource | Response | Promise<Response>): Promise<Checker> {
  const module =
    source instanceof Response || source instanceof Promise
      ? await instantiate(Promise.resolve(source))
      : await WebAssembly.instantiate(source, {})
  const exports = module.instance.exports as unknown as Exports
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()

  const call = (run: () => number, request: unknown): Reply => {
    const bytes = encoder.encode(JSON.stringify(request))
    const at = exports.input(bytes.length)
    // A fresh view each time: a request that grows the memory detaches the
    // buffer the previous view was over.
    new Uint8Array(exports.memory.buffer, at, bytes.length).set(bytes)
    const length = run()
    return JSON.parse(decoder.decode(new Uint8Array(exports.memory.buffer, exports.output(), length))) as Reply
  }

  return {
    check: (formula, turn) => {
      const reply = call(exports.check, { formula, turn })
      if ('checked' in reply) return reply.checked
      throw new Unchecked('error' in reply ? reply.error : 'the checker answered something else')
    },
    rate: (failures, easy) => {
      const reply = call(exports.rate, { failures, easy })
      if ('rating' in reply) return reply.rating
      throw new Unchecked('error' in reply ? reply.error : 'the checker answered something else')
    },
  }
}

/**
 * Streams the module when the server says what it is, and reads it whole
 * when it does not: a stand behind a proxy that labels `.wasm` as something
 * else still has a checker.
 */
async function instantiate(response: Promise<Response>): Promise<WebAssembly.WebAssemblyInstantiatedSource> {
  const fetched = await response
  if (!fetched.ok) throw new Error(`the checker could not be fetched: ${fetched.status}`)
  if (fetched.headers.get('content-type')?.startsWith('application/wasm')) {
    return WebAssembly.instantiateStreaming(fetched, {})
  }
  return WebAssembly.instantiate(await fetched.arrayBuffer(), {})
}

let loading: Promise<Checker> | null = null

/**
 * The app's checker, loaded once.
 *
 * Asked for when the app starts rather than at the first turn: fetched then,
 * it is in the worker's cache before the learner goes underground, and the
 * first turn of a sitting does not wait for it.
 */
export function checker(): Promise<Checker> {
  loading ??= checkerOf(fetch(wasmUrl)).catch((error: unknown) => {
    // A failed load is not kept: the next sitting tries again.
    loading = null
    throw error
  })
  return loading
}
