/** What the server answers with, and the calls that ask it. */

/** A pair: the learner's own language and the language being learnt. */
export interface Pair {
  /** The prompt, in the learner's own language. */
  native: string
  /** The answer, in the language being learnt. */
  target: string
}

/** A word marked in a sentence: where it stands, and how it is spelt there. */
export interface Mark {
  /** The word's id, `en:doctor`. */
  word: string
  lemma: string
  /** As spelt in the sentence: "went" for go. */
  form: string
  /** The character it starts at. */
  start: number
}

/** A worked example of a formula: a sentence, what it means, and its words. */
export interface Sample extends Pair {
  sentence: number
  words: Mark[]
  /** Other spellings just as right: "I am not hungry." for "I'm not hungry.". */
  accept: string[]
}

/** One filling for a slot, with the forms that agree with it. */
export interface Value extends Pair {
  /** What `say` picks from for agreement: `be` is "is" on "he". */
  forms: Record<string, string>
  /** Whether this is one of the learner's own words rather than the pack's. */
  own: boolean
}

/** What kind of the learner's own words a slot takes. */
export type OwnKind = 'noun' | 'verb'

/** A hole in a formula's pattern, with what can go in it. */
export interface Slot {
  name: string
  /** The kind of own words the slot takes besides its values, when any. */
  takes: OwnKind | null
  values: Value[]
}

/** A formula another stands on, and where it stands. */
export interface Parent {
  id: string
  name: string
  stitch: Stitch
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
  /** Other templates as right as `say`: "<pronoun>'d like <thing>.". */
  accept: string[]
  /** The formulas this one stands on; it opened once they were all sewn. */
  parents: Parent[]
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

/** One sentence a word stands in. */
export interface Context {
  sentence: number
  /** The sentence, in the language being learnt. */
  text: string
  /** What it means, in the learner's own. */
  translation: string
  /** How the word is spelt here. */
  form: string
  /** The character the word starts at. */
  start: number
  /** The formula whose example this is. */
  formula: string
  /** Whether this is the sentence the word is heard in. */
  anchor: boolean
}

/** A word: a lemma met inside sentences. */
export interface Word {
  /** `en:doctor`. */
  id: string
  /** As written: "doctor", "I", "Monday". */
  lemma: string
  /** What it means, in the learner's own language. */
  gloss: string
  /** How it sounds, IPA. */
  ipa: string | null
  /** Its place among the commonest words of the language, 1 the commonest. */
  rank: number | null
  /** The smallest level that holds it - 1000, 2000 or 5000 - or null past it. */
  level: number | null
  /** The sentences it has been met in, its anchor first. */
  contexts: Context[]
  languages: { native: string; target: string }
}

/** What every item in today's queue carries, whatever it is about. */
interface DueCommon {
  stitch: Stitch
  is_new: boolean
  due: string
  pace: Pace | null
  /** How firmly it holds, in days: FSRS's stability. 0 for a card never answered. Drawn as a bar. */
  stability: number
}

/** A formula, asked one way round. */
export type FormulaDue = DueCommon & { kind: 'formula'; formula: Formula; direction: Direction }

/** A word, asked in its sentence. */
export type WordDue = DueCommon & { kind: 'word'; word: Word }

/** One item in today's queue. */
export type Due = FormulaDue | WordDue

/** How many formulas or words stand in each state. */
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

/** One level - the commonest 1000, 2000 or 5000 words - and the learner's words in it. */
export interface Level {
  size: number
  basted: number
  sewn: number
}

/** Where the learner stands with words. */
export interface Words {
  /** The pack's words, by state. */
  counts: Counts
  /** Every word in hand against the levels, the pack's and those declared known. */
  levels: Level[]
  /** Met in a sentence and not started yet. */
  waiting: number
  /** Held on the learner's word alone: declared known and not asked since. */
  declared: number
}

/** The formula next in line, and the parents it is waiting on. */
export interface Next {
  id: string
  name: string
  /** Parents not yet sewn; empty when it waits only for a day with room. */
  waits_for: Parent[]
}

/** What a day has room for that is new, and what took the rest. */
export interface Room {
  /** Cards that had waited through a whole day they could have been answered on, as the day began. */
  debt: number
  /** New words the day opens, all told: ten, one fewer for every card of debt. */
  words: number
  /** New formulas the day opens: one while the debt leaves room for a word. */
  formulas: number
}

/** A break: the schedule's clock stopped while the learner is away. */
export interface Break {
  id: number
  starts_at: string
  /** When the learner is back; null until they say so. */
  ends_at: string | null
}

/** Last week's slips, as the first sitting of a week is told them. */
export interface Note {
  /** The Monday of the week, YYYY-MM-DD. */
  starts_on: string
  slipped: number
  /** The first few, by name. */
  names: string[]
}

/** Everything the Today screen shows. */
export interface Today {
  queue: Due[]
  reviewed_today: number
  counts: Counts
  progress: Progress
  words: Words
  /** The next formula not opening today, or null once all have started. */
  next: Next | null
  room: Room
  /** The break the learner is on; the queue is empty while there is one. */
  on_break: Break | null
  /** The last answer, when the time since it could be counted as a break. */
  away_since: string | null
  /** What slipped last week, while this week has had no answer yet. */
  last_week: Note | null
  /** When the tutor read this: a copy kept on the device says how old it is. */
  as_of: string
  /** How long an answer usually takes, by kind: what a short sitting is measured with. */
  usual: Usual
  /** When the cards not in the queue come back over the next week, earliest first. */
  coming: string[]
}

/** How long an answer usually takes, in milliseconds; null before there are enough. */
export interface Usual {
  produce: number | null
  recognise: number | null
  word: number | null
}

/** Today as the device read it: from the tutor, or from the copy kept for the road. */
export interface Read {
  today: Today
  /** True when the tutor was out of reach and this is the copy last read. */
  cached: boolean
  /** When the read began, by this device's clock. */
  readAt: number
}

/**
 * Put on an answer the service worker gave from its copy because the tutor
 * did not answer. Named in `public/sw.js` too.
 */
export const FROM_CACHE = 'x-hilvan-cached'

/** Something the learner had come up with, and then could not. */
export interface Slip {
  kind: 'formula' | 'word'
  id: string
  /** The formula's name, or the word's lemma. */
  name: string
  /** Which way round a formula slipped; null for a word. */
  direction: Direction | null
  /** What a word means; null for a formula. */
  gloss: string | null
  at: string
  /** Where it stands now. */
  stitch: Stitch
  /** Whether it slipped from the learner's declaration that they knew it. */
  declared: boolean
}

/** One week's slips. */
export interface Week {
  /** Its Monday, YYYY-MM-DD. */
  starts_on: string
  slips: Slip[]
}

/** What slipped this week and last. */
export interface Drift {
  this_week: Week
  last_week: Week
}

/** How the answer went. The four the drill offers. */
export type Rating = 'again' | 'hard' | 'good' | 'easy'

/** What an answer changed. */
export interface Reviewed {
  stitch: Stitch
  due: string
  interval_days: number
  pace: Pace | null
  /** The grade it was given: the learner's when heard, the turns' when typed. */
  rating: Rating
  /** Words whose failure was sent to their own cards. */
  words: string[]
  /** How firmly it holds now, and held before the answer, in days. */
  stability: number
  stability_before: number
}

/**
 * Which question a typed turn asked: one of the formula's own sentences, by
 * id, or a substitution, by the value put in each slot.
 */
export type Asked = { sample: number } | { values: Record<string, string> }

/** One typed turn. */
export interface Turn {
  asked: Asked
  typed: string
}

/** What went wrong in a typed answer. */
export type Failure = 'formula' | 'order' | 'word'

/** One word of the right answer, and whether the typed answer had it. */
export interface Expected {
  text: string
  /** A word the formula carries, rather than the formula's own. */
  word: boolean
  came: boolean
}

/** What the check of a typed turn found. */
export interface Checked {
  correct: boolean
  /** The formula's own answer, as it is heard. */
  answer: string
  /** The right answer closest to what was typed. */
  closest: string
  expected: Expected[]
  /** Words typed that the right answer does not have. */
  extra: string[]
  failure: Failure | null
  /** Words the shape carried that did not come. */
  missed: string[]
}

/** One of the learner's own words. */
export interface OwnWord {
  id: number
  kind: OwnKind
  native: string
  target: string
  /** Whether the lexicon knows every word of it - a hint at a typo. */
  known: boolean
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

/** A word the inventory asks about. */
export interface InventoryItem {
  /** `en:house`: what the answer is sent back under. */
  word: string
  lemma: string
  /** How it sounds, IPA, when the lexicon has it. */
  ipa: string | null
  /** Its place in the frequency list, 1 the commonest. */
  rank: number
}

/** One level, and how far the inventory has gone through it. */
export interface Reach {
  size: number
  /** Words of the level still to be asked. */
  left: number
  /** Words of the level answered "I know it". */
  known: number
}

/** Where the inventory stands, and what it asks next. */
export interface InventoryRun {
  /** The language being learnt; null before a pack is loaded. */
  language: string | null
  /** The next words to ask, commonest first. */
  next: InventoryItem[]
  levels: Reach[]
  /** Answers given, either way. */
  answered: number
  known: number
}

/** One answer of the inventory. */
export interface InventoryAnswer {
  word: string
  known: boolean
}

/** Where the learner stood at a moment, counted the way Today counts. */
export interface Snapshot {
  at: string
  progress: Progress
  words: Words
  /** Answers given by then, asked ones. */
  answers: number
}

/** Where the learner stood at a moment, beside where they stand now. */
export interface Standings {
  then: Snapshot
  now: Snapshot
  /** The first answer the tutor asked for, if any. */
  began: string | null
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

/** The tutor could not be reached, and nothing was kept to answer instead. */
export class Unreachable extends Error {
  constructor() {
    super('The tutor cannot be reached from here, and nothing was kept on this device to go on with.')
    this.name = 'Unreachable'
  }
}

async function respond(path: string, init?: RequestInit): Promise<Response> {
  let response: Response
  try {
    response = await fetch(`/api${path}`, {
      ...init,
      headers: init?.body ? { 'content-type': 'application/json', ...init.headers } : init?.headers,
    })
  } catch {
    throw new Unreachable()
  }
  if (response.status === 401) {
    throw new Unauthorized()
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { error?: string } | null
    throw new Error(body?.error ?? `the server answered ${response.status}`)
  }
  return response
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  return (await (await respond(path, init)).json()) as T
}

export const api = {
  session: () => call<Session>('/session'),
  logIn: (password: string) => call<Session>('/session', { method: 'POST', body: JSON.stringify({ password }) }),
  logOut: () => call<Session>('/session', { method: 'DELETE' }),
  /** Today, from the tutor or - out of its reach - from the copy the worker kept. */
  today: async (): Promise<Read> => {
    const readAt = Date.now()
    const response = await respond('/today')
    return { today: (await response.json()) as Today, cached: response.headers.get(FROM_CACHE) === '1', readAt }
  },
  formula: (id: string) => call<Formula>(`/formulas/${encodeURIComponent(id)}`),
  word: (id: string) => call<Word>(`/words/${encodeURIComponent(id)}`),
  voices: () => call<VoicesScreen>('/voices'),
  chooseVoice: (language: string, voice: Voice) =>
    call<Voice>(`/voices/${encodeURIComponent(language)}`, {
      method: 'PUT',
      body: JSON.stringify({ engine: voice.engine, voice: voice.id }),
    }),
  listen: () => call<{ sentences: Heard[] }>('/listen').then((body) => body.sentences),
  ownWords: () => call<{ words: OwnWord[] }>('/own-words').then((body) => body.words),
  addOwnWord: (kind: OwnKind, native: string, target: string) =>
    call<OwnWord>('/own-words', { method: 'POST', body: JSON.stringify({ kind, native, target }) }),
  removeOwnWord: (id: number) => remove(`/own-words/${id}`),
  inventory: () => call<InventoryRun>('/inventory'),
  /** Answers, oldest first; the run as it stands after them. */
  answerInventory: (answers: InventoryAnswer[]) =>
    call<InventoryRun>('/inventory', { method: 'POST', body: JSON.stringify({ answers }) }),
  /** Takes an answer back; the run as it stands after. */
  undoInventory: (word: string) => call<InventoryRun>(`/inventory/${encodeURIComponent(word)}`, { method: 'DELETE' }),
  /** Where the learner stood at `at`, an ISO moment, and stands now. */
  standing: (at: string) => call<Standings>(`/model/standing?${new URLSearchParams({ at }).toString()}`),
  /** Starts a break now, until `until` (an ISO moment) or until the learner is back. */
  takeBreak: (until: string | null) => call<Break>('/breaks', { method: 'POST', body: JSON.stringify({ until }) }),
  /** Counts the time since the last answer as a break that ended now. */
  countAway: () => call<Break>('/breaks/away', { method: 'POST' }),
  /** Ends the break going on: the learner is back. */
  endBreak: (id: number) => call<Break>(`/breaks/${id}/end`, { method: 'POST' }),
  /** Removes a break as if it had never been taken. */
  removeBreak: (id: number) => remove(`/breaks/${id}`),
  drift: () => call<Drift>('/drift'),
}

/**
 * Where the learner model downloads from: as it stands, or as it stood at
 * `at`. A URL rather than a call, so a link can save it as a file.
 */
export function modelUrl(at?: string): string {
  return at ? `/api/model?${new URLSearchParams({ at }).toString()}` : '/api/model'
}

/** A DELETE, which answers with no body. */
async function remove(path: string): Promise<void> {
  const response = await fetch(`/api${path}`, { method: 'DELETE' })
  if (response.status === 401) throw new Unauthorized()
  if (!response.ok) throw new Error(`the server answered ${response.status}`)
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
