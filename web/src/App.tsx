import { useEffect, useState } from 'react'

import { describe, fetchHealth, type ServerState } from '@/health'

/**
 * The shell, before there is anything to teach.
 *
 * One page in the line's theme: the name, the version this build carries, and
 * a line about the server it is talking to. The learner model, formulas,
 * repetition and lessons arrive in the versions that introduce them.
 */
export function App() {
  const [server, setServer] = useState<ServerState>({ kind: 'checking' })

  useEffect(() => {
    let cancelled = false
    void fetchHealth().then((state) => {
      if (!cancelled) setServer(state)
    })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <main className="mx-auto flex min-h-dvh max-w-2xl flex-col justify-center gap-6 px-6 py-12">
      <header className="flex items-baseline gap-3">
        <h1 className="text-3xl font-semibold tracking-tight text-text">Hello, hilvan</h1>
        <span className="rounded-md bg-soft px-2 py-0.5 font-mono text-xs text-dim">v{__APP_VERSION__}</span>
      </header>
      <p className="text-base leading-relaxed text-dim">
        A self-hosted language tutor: grammar formulas, spaced repetition and audio lessons compiled from what you
        already know. This is the shell; the formulas are next.
      </p>
      <p className="border-t border-line pt-4 text-sm text-dim" aria-live="polite">
        {describe(server)}
      </p>
    </main>
  )
}
