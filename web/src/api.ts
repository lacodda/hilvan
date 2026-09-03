/** What the server answers with, and the calls that ask it. */

/** A worked example, or one value that can go in a slot. */
export interface Sample {
  /** The prompt, in the learner's own language. */
  native: string
  /** The answer, in the language being learnt. */
  target: string
}

/** A hole in a formula's pattern, with what can go in it. */
export interface Slot {
  name: string
  values: Sample[]
}

/** A grammar formula: a shape sentences are assembled from. */
export interface Formula {
  id: string
  name: string
  pattern: string
  explanation: string
  samples: Sample[]
  slots: Slot[]
}

/** How far along a formula is - the basting stitch the product is named after. */
export type Stitch = 'new' | 'basted' | 'sewn'

/** One item in today's queue. */
export interface Due {
  formula: Formula
  stitch: Stitch
  is_new: boolean
  due: string
}

/** How many formulas stand in each state. */
export interface Counts {
  new: number
  basted: number
  sewn: number
}

/** Everything the Today screen shows. */
export interface Today {
  queue: Due[]
  reviewed_today: number
  counts: Counts
}

/** How the answer went. The four the drill offers. */
export type Rating = 'again' | 'hard' | 'good' | 'easy'

/** What an answer changed. */
export interface Reviewed {
  stitch: Stitch
  due: string
  interval_days: number
}

/** Whether the door is locked, and whether this browser is through it. */
export interface Session {
  required: boolean
  signed_in: boolean
}

/** A request the server turned away for want of a session. */
export class Unauthorized extends Error {
  constructor() {
    super('sign in first')
    this.name = 'Unauthorized'
  }
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    headers: init?.body ? { 'content-type': 'application/json', ...init.headers } : init?.headers,
  })
  if (response.status === 401) {
    throw new Unauthorized()
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { error?: string } | null
    throw new Error(body?.error ?? `the server answered ${response.status}`)
  }
  return (await response.json()) as T
}

export const api = {
  session: () => call<Session>('/session'),
  logIn: (password: string) => call<Session>('/session', { method: 'POST', body: JSON.stringify({ password }) }),
  logOut: () => call<Session>('/session', { method: 'DELETE' }),
  today: () => call<Today>('/today'),
  review: (id: string, rating: Rating, durationMs: number | null) =>
    call<Reviewed>(`/formulas/${encodeURIComponent(id)}/review`, {
      method: 'POST',
      body: JSON.stringify({ rating, duration_ms: durationMs }),
    }),
}
