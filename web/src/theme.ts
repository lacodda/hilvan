/**
 * Dark, light, or as the device is set.
 *
 * dowel's theme follows the system on its own; a choice pins a class on
 * `<html>` against it. Kept on this device only - a phone read under the
 * covers at night and a laptop by a window want different answers - and
 * applied before the first paint, so the app never flashes the other one.
 */

import { useSyncExternalStore } from 'react'

export type Theme = 'system' | 'dark' | 'light'

export const THEMES: readonly Theme[] = ['system', 'dark', 'light']

const STORAGE_KEY = 'hilvan.theme'
const CHANGED = 'hilvan:theme'

function stored(): Theme {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return THEMES.includes(raw as Theme) ? (raw as Theme) : 'system'
  } catch {
    // Storage refused - a private window, site data blocked: the system's.
    return 'system'
  }
}

function apply(theme: Theme): void {
  const root = document.documentElement
  root.classList.toggle('dark', theme === 'dark')
  root.classList.toggle('light', theme === 'light')
}

/** Applies the stored choice; called once, before React renders. */
export function initTheme(): void {
  apply(stored())
}

/** The choice, and a way to change it, shared by every screen that shows it. */
export function useTheme(): { theme: Theme; setTheme: (theme: Theme) => void } {
  const theme = useSyncExternalStore(
    (onChange) => {
      window.addEventListener(CHANGED, onChange)
      return () => window.removeEventListener(CHANGED, onChange)
    },
    stored,
    (): Theme => 'system',
  )
  const setTheme = (next: Theme) => {
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Not kept past this page; applied all the same.
    }
    apply(next)
    window.dispatchEvent(new Event(CHANGED))
  }
  return { theme, setTheme }
}
