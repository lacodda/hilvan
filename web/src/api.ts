/** What the server answers with, and the calls that ask it. */

/** A worked example, or one value that can go in a slot. */
export interface Sample {
  /** The prompt, in the learner's own language. */
  native: string
  /** The answer, in the language being learnt. */
  target: string
}

/** One filling for a slot, with the forms that agree with it. */
export interface Value extends Sample {
  /** What `say` picks from for agreement: `be` is "is" on "he". */
  forms: Record<string, string>
}

/** A hole in a formula's pattern, with what can go in it. */
export interface Slot {
  name: string
  values: Value[]
}

/** Which of the three ways a shape can be said. */
export type Form = 'statement' | 'negation' | 'question'

/** Another form of the same shape, as the switch on the card shows it. */
export interface Sister {
  id: string
  form: Form
  name: string
  pattern: string
  /** Where that form stands, so the switch can show what is still untouched. */
  stitch: Stitch
}

/** A grammar formula: a shape sentences are assembled from. */
export interface Formula {
  id: string
  name: string
  pattern: string
  /** The sentence a substitution says, with `<slot>` and `<slot:form>` holes. */
  say: string | null
  explanation: string
  samples: Sample[]
  slots: Slot[]
  /** The shape this is one form of, when it is one of several. */
  family: string | null
  form: Form | null
  /** The other forms of the same shape. Empty when the formula stands alone. */
  sisters: Sister[]
  /** Which language each side is in, ISO 639-1: what the voice is asked for. */
  languages: { native: string; target: string }
}

/** How far along a formula is - the basting stitch the product is named after. */
export type Stitch = 'new' | 'basted' | 'sewn'

/**
 * Which way round a formula is being asked.
 *
 * Producing is saying it: the prompt is in your own language. Recognising is
 * understanding it: the prompt is the English. They are scheduled apart,
 * because they are learnt apart.
 */
export type Direction = 'produce' | 'recognise'

/**
 * How fast a card usually comes.
 *
 * The second dimension of knowing something: stability says whether it is
 * still there, pace says whether it still costs thought. Shown, never used to
 * schedule.
 */
export interface Pace {
  /** The median of the last few answers, in milliseconds. */
  typical_ms: number
  /** The most recent answer. */
  last_ms: number
  /** How many timed answers the median rests on. */
  answers: number
}

/** One item in today's queue. */
export interface Due {
  formula: Formula
  direction: Direction
  stitch: Stitch
  is_new: boolean
  due: string
  pace: Pace | null
}

/** How many formulas stand in each state. */
export interface Counts {
  new: number
  basted: number
  sewn: number
}

/** The same standing told once per direction. */
export interface Progress {
  produce: Counts
  recognise: Counts
}

/** Everything the Today screen shows. */
export interface Today {
  queue: Due[]
  reviewed_today: number
  counts: Counts
  progress: Progress
}

/** How the answer went. The four the drill offers. */
export type Rating = 'again' | 'hard' | 'good' | 'easy'

/** What an answer changed. */
export interface Reviewed {
  stitch: Stitch
  due: string
  interval_days: number
  pace: Pace | null
}

/** How fast a sentence is said: slowly while it is new. */
export type Tempo = 'slow' | 'normal'

/** A voice the learner can choose. */
export interface Voice {
  engine: 'piper' | 'elevenlabs'
  id: string
  name: string
  /** Empty for a voice that speaks any language. */
  languages: string[]
}

/** One language as the voices screen shows it. */
export interface LanguageVoices {
  code: string
  /** `native` for the learner's own language, `target` for one being learnt. */
  role: 'native' | 'target'
  /** The voice it is spoken in now, and whether the learner chose it. */
  spoken: { voice: Voice; chosen: boolean } | null
  options: Voice[]
  /** A sentence of the material to hear a voice say. */
  sample: string | null
}

/** Whether an engine is configured and answering. */
export type EngineState = 'ok' | 'unreachable' | 'off'

/** What is left of the ElevenLabs budget, and whether it will last. */
export interface Budget {
  remaining: number
  limit: number
  resets_at: string | null
  per_day: number
  runs_out_at: string | null
  lasts: boolean
}

/** Everything the voices screen shows. */
export interface VoicesScreen {
  languages: LanguageVoices[]
  piper: EngineState
  elevenlabs: EngineState
  budget: Budget | null
}

/** A sentence of the listening mode. */
export interface Heard {
  formula: string
  /** What is heard. */
  target: string
  /** What it means. */
  native: string
  /** The language `target` is in. */
  language: string
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
  formula: (id: string) => call<Formula>(`/formulas/${encodeURIComponent(id)}`),
  review: (id: string, rating: Rating, direction: Direction, durationMs: number | null) =>
    call<Reviewed>(`/formulas/${encodeURIComponent(id)}/review`, {
      method: 'POST',
      body: JSON.stringify({ rating, direction, duration_ms: durationMs }),
    }),
  voices: () => call<VoicesScreen>('/voices'),
  chooseVoice: (language: string, voice: Voice) =>
    call<Voice>(`/voices/${encodeURIComponent(language)}`, {
      method: 'PUT',
      body: JSON.stringify({ engine: voice.engine, voice: voice.id }),
    }),
  listen: () => call<{ sentences: Heard[] }>('/listen').then((body) => body.sentences),
}

/**
 * Where a sentence of the material is spoken.
 *
 * A URL rather than a call: an audio element fetches it itself, with the
 * cookie, and the browser keeps the sound under its ETag.
 */
export function speechUrl(language: string, text: string, tempo: Tempo): string {
  const query = new URLSearchParams({ language, text, tempo })
  return `/api/speech?${query.toString()}`
}
