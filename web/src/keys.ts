/**
 * The drill from a keyboard: 1 to 4 grade, the space bar does the one thing
 * the screen is waiting for - reveal, next, grade it.
 *
 * Read by `event.code`, the key's place on the keyboard, not by
 * `event.key`, the character it types: a learner of English types with a
 * Russian layout half the time, and on it the "1" key is still "1" but a
 * letter key is not the letter. A key pressed in a field types into it, and
 * a key with a modifier belongs to the browser.
 */

import { useEffect, useRef } from 'react'

/** The keys the drill listens to, by what they mean. */
export type Key = 'space' | '1' | '2' | '3' | '4'

/** What a key event means to the drill, if anything. */
export function keyOf(event: Pick<KeyboardEvent, 'code' | 'altKey' | 'ctrlKey' | 'metaKey' | 'target'>): Key | null {
  if (event.altKey || event.ctrlKey || event.metaKey) return null
  const target = event.target as { tagName?: string; isContentEditable?: boolean } | null
  if (target && (target.isContentEditable || target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT')) {
    return null
  }
  switch (event.code) {
    case 'Space':
      return 'space'
    case 'Digit1':
    case 'Numpad1':
      return '1'
    case 'Digit2':
    case 'Numpad2':
      return '2'
    case 'Digit3':
    case 'Numpad3':
      return '3'
    case 'Digit4':
    case 'Numpad4':
      return '4'
    default:
      return null
  }
}

/**
 * Listens for the drill's keys while the screen is mounted. `keys` maps a
 * key to what it does now; a key missing from it does nothing, so a screen
 * passes only what makes sense in its current phase.
 */
export function useKeys(keys: Partial<Record<Key, () => void>>): void {
  // The latest map, read when a key comes: the listener is added once rather
  // than on every render the phase changes in.
  const current = useRef(keys)
  useEffect(() => {
    current.current = keys
  })
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      const key = keyOf(event)
      if (key === null) return
      const run = current.current[key]
      if (!run) return
      // The space bar would otherwise press the focused button as well, or
      // scroll the page.
      event.preventDefault()
      run()
    }
    window.addEventListener('keydown', listener)
    return () => window.removeEventListener('keydown', listener)
  }, [])
}

/** The rating each number key gives. */
export const RATING_KEYS = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' } as const
