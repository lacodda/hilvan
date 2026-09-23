/**
 * The sound of the drill: which side of a card is in which language, which
 * sides can be spoken at all, how fast, and one player for all of it.
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react'

import { speechUrl, type Direction, type Formula, type Stitch, type Tempo } from '@/api'
import type { Prompt } from '@/prompts'

/**
 * How fast a card is said: slowly while it is new, so every sound is heard,
 * and at the speed people talk once it is basted - that is the speed it will
 * be heard at.
 */
export function tempoFor(stitch: Stitch | null, isNew: boolean): Tempo {
  return isNew || stitch === 'new' ? 'slow' : 'normal'
}

/** One side of a card, as the voice sees it. */
export interface Side {
  text: string
  language: string
  /** Whether there is a sentence here to say. */
  speakable: boolean
}

/**
 * The question and the answer of a turn, each with its language.
 *
 * The side in the language being learnt is always a sentence - a sample, or
 * what `say` made of the substitution. The side in the learner's own language
 * is a sentence only for a sample: for a substitution it is the scaffold
 * ("я + am/is/are + дома"), and reading a scaffold aloud is noise the server
 * refuses anyway.
 */
export function sides(prompt: Prompt, direction: Direction, formula: Pick<Formula, 'languages'>): { question: Side; answer: Side } {
  const native: Side = { text: prompt.native, language: formula.languages.native, speakable: prompt.source === 'sample' }
  const target: Side = { text: prompt.target, language: formula.languages.target, speakable: true }
  return direction === 'produce' ? { question: native, answer: target } : { question: target, answer: native }
}

/** The URL of a side, or `null` when it is not a sentence. */
export function urlOf(side: Side, tempo: Tempo): string | null {
  return side.speakable ? speechUrl(side.language, side.text, tempo) : null
}

/**
 * One player for the whole app.
 *
 * A single audio element, so a new sentence stops the one before instead of
 * talking over it, and so a phone that allowed sound once keeps allowing it.
 */
class Player {
  private readonly audio = typeof Audio === 'undefined' ? null : new Audio()
  private current: string | null = null
  private failed = new Set<string>()
  private listeners = new Set<() => void>()
  private sequence = 0

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** What is playing now, by URL. */
  playing = () => this.current

  /** Whether a URL could not be played - no voice, or the engine failed. */
  hasFailed = (url: string) => this.failed.has(url)

  private set(current: string | null) {
    this.current = current
    for (const listener of this.listeners) listener()
  }

  /** Settles the sound in flight, if any, as ended. */
  private finish: (() => void) | null = null

  /** Plays one sound, resolving when it ends, is replaced, is stopped or fails. */
  play(url: string): Promise<void> {
    const audio = this.audio
    if (!audio) return Promise.resolve()
    this.finish?.()
    const ticket = ++this.sequence
    audio.pause()
    audio.src = url
    this.failed.delete(url)
    this.set(url)
    return new Promise((resolve) => {
      const done = (failed: boolean) => {
        // Recorded before anyone is told, so a listener woken below already
        // sees the failure.
        if (failed) this.failed.add(url)
        if (this.sequence === ticket) {
          audio.onended = null
          audio.onerror = null
          this.finish = null
          this.set(null)
        }
        resolve()
      }
      this.finish = () => done(false)
      audio.onended = () => done(false)
      audio.onerror = () => done(true)
      audio.play().catch(() => done(true))
    })
  }

  /** Plays several sounds one after the other; any other play, or a stop, ends the run. */
  async playAll(urls: string[], gapMs = 400): Promise<void> {
    for (const [index, url] of urls.entries()) {
      if (index > 0) {
        const before = this.sequence
        await new Promise((resolve) => setTimeout(resolve, gapMs))
        if (this.sequence !== before) return
      }
      const played = this.play(url)
      const mine = this.sequence
      await played
      if (this.sequence !== mine) return
    }
  }

  stop() {
    this.sequence++
    this.finish?.()
    this.finish = null
    this.audio?.pause()
    this.set(null)
  }
}

export const player = new Player()

/** What is playing now, as React state. */
export function usePlaying(): string | null {
  return useSyncExternalStore(player.subscribe, player.playing, () => null)
}

/** Stops the voice when the screen that started it goes away. */
export function useStopOnLeave() {
  useEffect(() => () => player.stop(), [])
}

/**
 * The learner's own voice, recorded next to the sample.
 *
 * Kept in memory and nowhere else: the recording is gone when the card is,
 * which is the whole promise - practice, not a file.
 */
export function useRecorder() {
  const [state, setState] = useState<'idle' | 'recording' | 'recorded' | 'refused'>('idle')
  const [url, setUrl] = useState<string | null>(null)
  const [recorder, setRecorder] = useState<MediaRecorder | null>(null)

  const available = typeof window !== 'undefined' && window.isSecureContext && Boolean(navigator.mediaDevices?.getUserMedia)

  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url)
    },
    [url],
  )

  const start = useCallback(() => {
    player.stop()
    void navigator.mediaDevices
      .getUserMedia({ audio: true })
      .then((stream) => {
        const chunks: Blob[] = []
        const next = new MediaRecorder(stream)
        next.ondataavailable = (event) => chunks.push(event.data)
        next.onstop = () => {
          for (const track of stream.getTracks()) track.stop()
          setUrl(URL.createObjectURL(new Blob(chunks, { type: next.mimeType })))
          setState('recorded')
        }
        next.start()
        setRecorder(next)
        setState('recording')
      })
      .catch(() => setState('refused'))
  }, [])

  const stop = useCallback(() => {
    recorder?.stop()
    setRecorder(null)
  }, [recorder])

  return { available, state, url, start, stop }
}
