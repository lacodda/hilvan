import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Sending } from '@/outbox'

// The line's storage is IndexedDB, which this environment does not have. What
// is under test is the delivering: the order answers reach the tutor in, what
// happens when it goes away mid-way, and what becomes of an answer it
// refuses. Those are the rules a day of answers underground rests on.
const store = vi.hoisted(() => ({ line: [] as Sending[] }))

vi.mock('@/outbox', () => ({
  pending: () => Promise.resolve(store.line.filter((sending) => sending.landedAt === undefined)),
  land: (sending: Sending, at: number) => {
    store.line = store.line.map((kept) => (kept.id === sending.id ? { ...kept, landedAt: at } : kept))
    return Promise.resolve()
  },
  forget: (sending: Sending) => {
    store.line = store.line.filter((kept) => kept.id !== sending.id)
    return Promise.resolve()
  },
  unkept: () => 0,
}))

const { delivered, deliveryState, drain, sawTutor } = await import('@/delivery')

function answer(id: number): Sending {
  return {
    id,
    answerId: `a${id}`,
    path: `/words/en:w${id}/review`,
    body: { answer_id: `a${id}`, answered_at: '2026-10-20T08:00:00.000Z', rating: 'good' },
    card: `word:en:w${id}`,
    answeredAt: id,
  }
}

/** A fetch that answers with the given statuses, in call order; the last one repeats. */
function answering(...statuses: (number | 'unreachable')[]) {
  const calls: string[] = []
  let at = 0
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      calls.push(url)
      const status = statuses[Math.min(at, statuses.length - 1)] ?? 200
      at += 1
      if (status === 'unreachable') return Promise.reject(new Error('underground'))
      const body = status === 200 ? JSON.stringify({ stitch: 'basted', interval_days: 2 }) : '{}'
      return Promise.resolve(new Response(body, { status, headers: { 'content-type': 'application/json' } }))
    }),
  )
  return calls
}

beforeEach(async () => {
  store.line = []
  sawTutor(true)
  await drain()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the line of answers', () => {
  it('delivers oldest first, and keeps what landed until a fresh Today counts it', async () => {
    store.line = [answer(1), answer(2), answer(3)]
    const calls = answering(200)
    await drain()
    expect(calls).toEqual(['/api/words/en:w1/review', '/api/words/en:w2/review', '/api/words/en:w3/review'])
    expect(store.line.every((sending) => sending.landedAt !== undefined)).toBe(true)
    expect(deliveryState().waiting).toBe(0)
  })

  it('stops at the first answer that cannot be delivered, and keeps everything behind it', async () => {
    store.line = [answer(1), answer(2), answer(3)]
    const calls = answering(200, 'unreachable')
    await drain()
    expect(calls).toHaveLength(2)
    expect(store.line.filter((sending) => sending.landedAt === undefined).map((sending) => sending.id)).toEqual([2, 3])
    expect(deliveryState()).toMatchObject({ reachable: false, waiting: 2 })
  })

  it('waits on a tutor having a bad moment rather than dropping the answer', async () => {
    store.line = [answer(1), answer(2)]
    answering(503)
    await drain()
    expect(store.line.filter((sending) => sending.landedAt === undefined)).toHaveLength(2)
  })

  it('keeps the answers of a device signed out, for the sign-in', async () => {
    store.line = [answer(1)]
    answering(401)
    await drain()
    expect(store.line).toHaveLength(1)
    expect(deliveryState()).toMatchObject({ reachable: true, signIn: true, waiting: 1 })
  })

  it('drops an answer the tutor refuses and goes on with the rest', async () => {
    store.line = [answer(1), answer(2)]
    const calls = answering(404, 200)
    const before = deliveryState().refused
    await drain()
    expect(calls).toHaveLength(2)
    expect(store.line.map((sending) => sending.id)).toEqual([2])
    expect(deliveryState().refused).toBe(before + 1)
  })

  it('hands a screen the reply to its answer when the tutor answers in time', async () => {
    store.line = [answer(1)]
    answering(200)
    expect(await delivered('a1', 1_000)).toMatchObject({ stitch: 'basted' })
  })

  it('tells a screen its answer is kept when the tutor cannot be reached', async () => {
    store.line = [answer(1)]
    answering('unreachable')
    expect(await delivered('a1', 1_000)).toBeNull()
    expect(store.line[0]!.landedAt).toBeUndefined()
  })

  it('takes an answer that joined the line while a drain was already under way', async () => {
    store.line = [answer(1)]
    let release: () => void = () => undefined
    const slow = new Promise<void>((resolve) => {
      release = resolve
    })
    const calls: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        calls.push(url)
        if (calls.length === 1) await slow
        return new Response(JSON.stringify({ stitch: 'sewn' }), { status: 200 })
      }),
    )
    const first = drain()
    store.line.push(answer(2))
    const reply = delivered('a2', 2_000)
    release()
    await first
    expect(await reply).toMatchObject({ stitch: 'sewn' })
    expect(calls).toEqual(['/api/words/en:w1/review', '/api/words/en:w2/review'])
  })
})
