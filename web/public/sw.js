/**
 * The tutor out of reach of the tutor.
 *
 * Three caches, because the three things live differently. The shell - the
 * app itself - is replaced wholesale when a new version is deployed. The
 * study reads - Today, and the formulas and words it hands over - are the
 * learner's day as the tutor last said it, kept across app updates. The
 * sounds are named by their text and speed, and a sound kept once is good
 * until the voice changes.
 *
 * Nothing here is generated: the shell is fetched network-first and falls
 * back to the cache, so no list of hashed asset names can strand the app on
 * an old build. The page fills the caches itself (`src/offline.ts`); this
 * worker answers from them when the tutor does not.
 *
 * The version below is replaced at build time with the app's own, so a
 * deploy retires the previous shell.
 */

const VERSION = '__APP_VERSION__'
const SHELL = `hilvan-shell-${VERSION}`
/** Named again in `src/offline.ts`. */
const STUDY = 'hilvan-study'
const SOUND = 'hilvan-sound'

/**
 * Put on an answer given from a cache because the tutor did not answer: the
 * app reads it as "out of reach", not as the tutor being there. Named in
 * `src/api.ts` as `FROM_CACHE`.
 */
const FROM_CACHE = 'x-hilvan-cached'

/** Where the page leaves the number for the icon (`src/offline.ts`). */
const BADGE = '/__badge'

const ENTRY = '/index.html'

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(SHELL)
      .then((cache) => cache.add(ENTRY))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(names.filter((name) => name.startsWith('hilvan-shell-') && name !== SHELL).map((name) => caches.delete(name))))
      .then(() => self.clients.claim()),
  )
})

/** The reads a sitting needs, kept as the tutor last answered them. */
function isStudyRead(url) {
  return url.pathname === '/api/today' || url.pathname.startsWith('/api/formulas/') || url.pathname.startsWith('/api/words/')
}

function isSound(url) {
  return url.pathname === '/api/speech'
}

/** A cached answer, marked as one. */
function fromCache(response) {
  const headers = new Headers(response.headers)
  headers.set(FROM_CACHE, '1')
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers })
}

/**
 * From the network, falling back to what was kept.
 *
 * Network first: at home the learner sees the tutor as it is now, and the
 * copy is what makes the train work. A read that succeeds refreshes the
 * copy on the way past; one the tutor refuses for want of a session takes
 * the copy away - a device signed out does not keep the learner's day.
 */
async function freshestFirst(request, cacheName, { mark }) {
  const cache = await caches.open(cacheName)
  try {
    const response = await fetch(request)
    if (response.ok) {
      await cache.put(request, response.clone())
    } else if (response.status === 401 || response.status === 403) {
      await cache.delete(request)
    }
    return response
  } catch (unreachable) {
    const cached = await cache.match(request)
    if (cached) return mark ? fromCache(cached) : cached
    throw unreachable
  }
}

self.addEventListener('fetch', (event) => {
  const { request } = event
  if (request.method !== 'GET') return
  const url = new URL(request.url)
  if (url.origin !== self.location.origin) return

  if (isStudyRead(url)) {
    event.respondWith(freshestFirst(request, STUDY, { mark: true }))
    return
  }
  if (isSound(url)) {
    event.respondWith(freshestFirst(request, SOUND, { mark: false }))
    return
  }
  // The rest of the API is about the tutor rather than for a sitting - its
  // health, the session, the voices, the learner model - and an old copy of
  // any of it would answer about a moment that has passed.
  if (url.pathname.startsWith('/api/')) return

  // A navigation is answered with the app itself: every route is the same
  // document, and the app opened from the home screen underground has to
  // reach it.
  if (request.mode === 'navigate') {
    event.respondWith(freshestFirst(new Request(ENTRY), SHELL, { mark: false }).catch(() => caches.match(ENTRY)))
    return
  }
  event.respondWith(freshestFirst(request, SHELL, { mark: false }))
})

/**
 * The number on the icon, recounted while the app is closed: the reviews the
 * page left waiting, and the cards that have come back since.
 */
async function recount() {
  const kept = await (await caches.open(STUDY)).match(BADGE)
  if (!kept || !self.navigator.setAppBadge) return
  const { waiting, coming } = await kept.json()
  const now = Date.now()
  const count = waiting + coming.filter((at) => Date.parse(at) <= now).length
  if (count > 0) await self.navigator.setAppBadge(count)
  else await self.navigator.clearAppBadge()
}

self.addEventListener('periodicsync', (event) => {
  if (event.tag === 'badge') event.waitUntil(recount().catch(() => undefined))
})
