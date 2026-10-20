/**
 * What the device keeps so that a sitting works out of the tutor's reach:
 * the app itself, the latest Today and the formulas around it, and the
 * sounds the sitting will play. The service worker (`public/sw.js`) answers
 * from these when the tutor does not; this module fills them while it does.
 *
 * Filled from the page, through the Cache API, rather than by asking the
 * worker: on the first visit the worker does not yet control the page, and
 * a fill that went through it would quietly keep nothing - the defect
 * rhapsod's road found on its first cold visit.
 *
 * A worker needs a secure context: the tutor's https:// address, or
 * localhost. Over plain http:// none of this happens, and the screens say so.
 */

/** The caches, named as `public/sw.js` names them. */
const SHELL = `hilvan-shell-${__APP_VERSION__}`
const STUDY = 'hilvan-study'
const SOUND = 'hilvan-sound'

/** Where the worker finds the number for the icon. */
const BADGE = '/__badge'

/** Whether this browser, on this address, can keep anything for the road. */
export function canHold(): boolean {
  return typeof window !== 'undefined' && window.isSecureContext && 'serviceWorker' in navigator && 'caches' in window
}

/** Starts the worker. A build served by Vite's dev server has none. */
export function startWorker(): void {
  if (!canHold() || !import.meta.env.PROD) return
  void navigator.serviceWorker
    .register('/sw.js')
    .then(() => askForBadgeTicks())
    // A worker that will not start leaves an app that works in reach.
    .catch(() => undefined)
}

/**
 * Keeps the app itself: the page and every file it loaded - script, styles,
 * the checker. The worker keeps what passes through it, but on the first
 * visit nothing does, and a learner who opened the app once at home and
 * then went underground would have nothing to open.
 */
export async function holdShell(): Promise<void> {
  if (!canHold()) return
  const own = performance
    .getEntriesByType('resource')
    .map((entry) => new URL(entry.name))
    .filter((url) => url.origin === location.origin && url.pathname.startsWith('/assets/'))
    .map((url) => url.pathname)
  try {
    const cache = await caches.open(SHELL)
    await cache.addAll(['/index.html', '/manifest.webmanifest', '/favicon.svg', ...new Set(own)])
  } catch {
    // Out of reach mid-way: the worker keeps the rest as it passes through.
  }
}

/** What a fill got, and what it did not. */
export interface Filled {
  held: number
  missed: number
}

/**
 * Keeps each of `paths` in `cacheName`, one after another, skipping what is
 * kept already: background work with no deadline, that must not fight the
 * learner's own requests for the connection. Stops at the first sign the
 * tutor went away; what it got stays.
 */
async function fill(cacheName: string, paths: readonly string[], refresh: boolean): Promise<Filled> {
  if (!canHold() || paths.length === 0) return { held: 0, missed: paths.length }
  const cache = await caches.open(cacheName)
  let held = 0
  for (const path of paths) {
    if (!refresh && (await cache.match(path))) {
      held += 1
      continue
    }
    try {
      const response = await fetch(path)
      if (response.ok) {
        await cache.put(path, response)
        held += 1
      }
    } catch {
      // The tutor went away mid-fill: the rest is not tried against a tutor
      // that is not there, and the next read in reach carries on from here.
      break
    }
  }
  return { held, missed: paths.length - held }
}

/** Keeps the sounds a sitting will play. A sound kept once is kept: its URL names its text and speed. */
export function holdSounds(urls: readonly string[]): Promise<Filled> {
  return fill(SOUND, urls, false)
}

/** Keeps reads the sitting may need besides Today: the forms of a shape the learner can switch to. */
export function holdReads(paths: readonly string[]): Promise<Filled> {
  return fill(STUDY, paths, true)
}

/** How many of `urls` are kept on the device. */
export async function heldOf(urls: readonly string[]): Promise<number> {
  if (!canHold()) return 0
  const cache = await caches.open(SOUND)
  let held = 0
  for (const url of urls) if (await cache.match(url)) held += 1
  return held
}

/** Forgets everything kept for the road: the device is signed out, and the learner's material goes with it. */
export async function forgetHeld(): Promise<void> {
  if (!canHold()) return
  await Promise.all([caches.delete(STUDY), caches.delete(SOUND)])
}

/**
 * Puts the number on the app's icon, and leaves the worker what it needs to
 * keep it honest while the app is closed: the reviews waiting now, and when
 * the cards out of the queue come back.
 */
export async function setBadge(waiting: number, coming: readonly string[], now: number): Promise<void> {
  const count = waiting + coming.filter((at) => Date.parse(at) <= now).length
  try {
    if (count > 0) await navigator.setAppBadge?.(count)
    else await navigator.clearAppBadge?.()
  } catch {
    // No icon to put a number on: a browser tab, or a platform without badges.
  }
  if (!canHold()) return
  try {
    const cache = await caches.open(STUDY)
    await cache.put(BADGE, new Response(JSON.stringify({ waiting, coming }), { headers: { 'content-type': 'application/json' } }))
  } catch {
    // The worker recounts from the last record it has.
  }
}

/**
 * Asks for the worker to be woken now and then while the app is closed, to
 * recount the number on the icon as cards come back. Granted only to an
 * installed app, and only where the browser has it at all; elsewhere the
 * number is the one the app last left.
 */
async function askForBadgeTicks(): Promise<void> {
  try {
    const registration = await navigator.serviceWorker.ready
    const ticks = (registration as ServiceWorkerRegistration & { periodicSync?: { register: (tag: string, options: { minInterval: number }) => Promise<void> } })
      .periodicSync
    if (!ticks) return
    const permission = await navigator.permissions.query({ name: 'periodic-background-sync' as PermissionName })
    if (permission.state !== 'granted') return
    await ticks.register('badge', { minInterval: 60 * 60 * 1000 })
  } catch {
    // Without ticks the number is the one the app last left.
  }
}
