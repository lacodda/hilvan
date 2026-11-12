import { afterEach, describe, expect, it, vi } from 'vitest'

import { holdSounds, languageOf } from '@/offline'

/** A cache that keeps what it is given, in a map. */
function memoryCaches() {
  const kept = new Map<string, Response>()
  const cache = {
    match: (path: string) => Promise.resolve(kept.get(path)),
    put: (path: string, response: Response) => {
      kept.set(path, response)
      return Promise.resolve()
    },
  }
  return { kept, caches: { open: () => Promise.resolve(cache) } }
}

describe('keeping the sounds of a sitting', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reads the language a sound is asked in', () => {
    expect(languageOf('/api/speech?language=en&text=Hi.&tempo=slow')).toBe('en')
    expect(languageOf('/api/formulas/be')).toBeNull()
  })

  it('stops asking for a language the stand has no voice for at its first 404', async () => {
    const { kept, caches } = memoryCaches()
    vi.stubGlobal('window', { isSecureContext: true, caches })
    vi.stubGlobal('navigator', { serviceWorker: {} })
    vi.stubGlobal('caches', caches)
    const asked: string[] = []
    vi.stubGlobal('fetch', (path: string) => {
      asked.push(path)
      return Promise.resolve(new Response('', { status: languageOf(path) === 'ru' ? 404 : 200 }))
    })

    const urls = ['ru:one', 'en:two', 'ru:three', 'ru:four'].map((key) => {
      const [language, text] = key.split(':')
      return `/api/speech?language=${language}&text=${text}&tempo=normal`
    })
    const filled = await holdSounds(urls)

    expect(asked).toEqual([urls[0], urls[1]])
    expect(filled).toEqual({ held: 1, missed: 3 })
    expect([...kept.keys()]).toEqual([urls[1]])
  })
})
