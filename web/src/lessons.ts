/**
 * An audio lesson on this device: where in it the learner is, what they
 * have heard of it, and the file itself, kept for the road.
 *
 * The file is downloaded whole and played from here, never streamed: a walk
 * leaves the home network within minutes, and a lesson half fetched is a
 * lesson that stops in the park. It lives in a cache of its own, apart from
 * the app's - a new version of the app does not throw a lesson away - and
 * only the latest is kept.
 */

import { lessonAudioUrl, lessonPath, type Lesson, type Stretch } from '@/api'

const LESSONS = 'hilvan-lessons'

/** Where a lesson's description is kept beside its file. */
const describedAt = (id: number): string => `/api${lessonPath(id)}`

/** The stretch playing at `ms`: the last one started by then. */
export function stretchAt(chapters: readonly Stretch[], ms: number): number {
  let at = 0
  chapters.forEach((stretch, index) => {
    if (stretch.start_ms <= ms) at = index
  })
  return at
}

/** Where "next" goes from `ms`: the start of the stretch after the one playing. `null` at the last. */
export function nextStart(chapters: readonly Stretch[], ms: number): number | null {
  const next = chapters[stretchAt(chapters, ms) + 1]
  return next ? next.start_ms : null
}

/**
 * Where "back" goes from `ms`: the start of the stretch playing, or - in the
 * first two seconds of it - the one before, as a player's back button does.
 */
export function previousStart(chapters: readonly Stretch[], ms: number): number {
  const at = stretchAt(chapters, ms)
  const current = chapters[at]
  if (!current) return 0
  if (ms - current.start_ms > 2_000 || at === 0) return current.start_ms
  return chapters[at - 1]!.start_ms
}

/**
 * The phrases heard by the time `ms` has been played through: those asked
 * at least once to the end. A fresh phrase only heard and not yet asked has
 * not been answered, so it is not among them.
 */
export function askedBy(chapters: readonly Stretch[], ms: number): number[] {
  const asked = new Set<number>()
  for (const stretch of chapters) {
    if (stretch.turn === 'asked' && stretch.end_ms <= ms) asked.add(stretch.phrase)
  }
  return [...asked].sort((a, b) => a - b)
}

/** What this device remembers of one lesson. */
export interface Listened {
  /** How far into it, in milliseconds. */
  at: number
  /** Each phrase heard to the end of an asking, with when, by this device's clock. */
  heard: Record<number, number>
  /** Whether the learner has said how it went: then it is done with. */
  graded: boolean
}

const listenedKey = (id: number): string => `hilvan-lesson-${id}`

/** What this device remembers of a lesson; nothing for one never played here. */
export function listened(id: number): Listened {
  try {
    const kept = localStorage.getItem(listenedKey(id))
    if (kept) return JSON.parse(kept) as Listened
  } catch {
    // Storage refused, or what was there is not ours: start from the top.
  }
  return { at: 0, heard: {}, graded: false }
}

/** Keeps where a lesson stands on this device. */
export function remember(id: number, state: Listened): void {
  try {
    localStorage.setItem(listenedKey(id), JSON.stringify(state))
  } catch {
    // Kept for this page only; the lesson still plays.
  }
}

/**
 * Lays what has been played up to `ms` over what was remembered: each phrase
 * asked to the end by then is heard, at `now` unless it was heard before.
 * The place moves by whole seconds: a player reports it four times a
 * second, and the device need not write it down that often.
 */
export function hear(state: Listened, chapters: readonly Stretch[], ms: number, now: number): Listened {
  const heard = { ...state.heard }
  let changed = false
  for (const phrase of askedBy(chapters, ms)) {
    if (heard[phrase] === undefined) {
      heard[phrase] = now
      changed = true
    }
  }
  return changed || Math.abs(ms - state.at) >= 1_000 ? { ...state, at: ms, heard } : state
}

/** A lesson on the device: what it says, and its file. */
export interface Kept {
  lesson: Lesson
  audio: Blob
}

/**
 * When the browser keeps no caches - a page over plain HTTP - the lesson is
 * held in this page for as long as it is open.
 */
let inPage: Kept | null = null

const cachesHere = (): boolean => typeof caches !== 'undefined'

/** Takes a lesson onto the device, file and all, and lets the one before it go. */
export async function keep(lesson: Lesson): Promise<Kept> {
  const response = await fetch(lessonAudioUrl(lesson.id), { credentials: 'same-origin' })
  if (!response.ok) throw new Error(`the lesson's file could not be fetched: the tutor answered ${response.status}`)
  const audio = await response.blob()
  const kept = { lesson, audio }
  if (!cachesHere()) {
    inPage = kept
    return kept
  }
  const cache = await caches.open(LESSONS)
  await cache.put(lessonAudioUrl(lesson.id), new Response(audio, { headers: { 'content-type': 'audio/mpeg' } }))
  await cache.put(describedAt(lesson.id), new Response(JSON.stringify(lesson), { headers: { 'content-type': 'application/json' } }))
  const mine = new Set([new URL(lessonAudioUrl(lesson.id), location.href).href, new URL(describedAt(lesson.id), location.href).href])
  for (const request of await cache.keys()) {
    if (!mine.has(request.url)) await cache.delete(request)
  }
  return kept
}

/** The lesson on the device, if there is one. */
export async function kept(): Promise<Kept | null> {
  if (!cachesHere()) return inPage
  try {
    const cache = await caches.open(LESSONS)
    for (const request of await cache.keys()) {
      const path = new URL(request.url).pathname
      const id = /^\/api\/lessons\/(\d+)$/.exec(path)?.[1]
      if (id === undefined) continue
      const [described, audio] = await Promise.all([cache.match(request), cache.match(lessonAudioUrl(Number(id)))])
      if (!described || !audio) continue
      return { lesson: (await described.json()) as Lesson, audio: await audio.blob() }
    }
  } catch {
    // A cache that cannot be read holds nothing to play.
  }
  return null
}

/** How long a lesson lasts, as a person says it: "24 min". */
export function minutesLine(ms: number): string {
  return `${Math.max(1, Math.round(ms / 60_000))} min`
}

/** A moment in a lesson as a player shows it: "12:05". */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1_000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}
