import { Component, use, useCallback, useState, Suspense, type ReactNode } from 'react'

import { api, Unauthorized, type Today } from '@/api'
import { Drill } from '@/Drill'
import { SignIn } from '@/SignIn'
import { TodayScreen } from '@/TodayScreen'

/** What is on screen once the queue has been read. */
type View = { kind: 'today' } | { kind: 'drilling'; index: number }

/**
 * The tutor.
 *
 * Three screens and one loop: what is waiting today, the drill that works
 * through it, and the door when there is a password on it.
 */
export function App() {
  // The queue is a promise held in state rather than a fetch inside an
  // effect: React unwraps it with `use`, Suspense covers the wait and the
  // boundary below covers the failure, so there is no loading flag and no
  // error flag to keep in step with each other.
  const [queue, setQueue] = useState(() => api.today())
  const reload = useCallback(() => setQueue(api.today()), [])

  return (
    <Boundary reload={reload} queue={queue}>
      <Suspense fallback={<Shell>Opening the tutor…</Shell>}>
        <Sitting queue={queue} reload={reload} />
      </Suspense>
    </Boundary>
  )
}

function Sitting({ queue, reload }: { queue: Promise<Today>; reload: () => void }) {
  const [view, setView] = useState<View>({ kind: 'today' })
  const today = use(queue)

  if (view.kind === 'drilling') {
    const due = today.queue[view.index]
    if (!due) {
      // The sitting ended. Re-read the queue so the counts on Today are the
      // server's rather than a guess made here.
      setView({ kind: 'today' })
      reload()
      return <Shell>Saving…</Shell>
    }
    return (
      <Drill
        // A new formula is a new drill, not the same one reset: the key does
        // what an effect full of setState would otherwise have to. The
        // direction is part of it, because the same formula asked backwards
        // is a different question and must not inherit the state of the one
        // before it.
        key={`${due.formula.id}:${due.direction}`}
        due={due}
        position={view.index + 1}
        total={today.queue.length}
        onAnswered={() => setView({ kind: 'drilling', index: view.index + 1 })}
        onLeave={() => {
          setView({ kind: 'today' })
          reload()
        }}
      />
    )
  }

  return (
    <TodayScreen
      today={today}
      onStart={() => setView({ kind: 'drilling', index: 0 })}
      onSignOut={() => void api.logOut().then(reload)}
    />
  )
}

/**
 * Where a failed request lands.
 *
 * Being turned away is not an error to show the learner - it is the login
 * screen. Anything else is said plainly, with the one button that can help.
 */
class Boundary extends Component<
  { children: ReactNode; queue: Promise<Today>; reload: () => void },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error : new Error(String(error)) }
  }

  componentDidUpdate(previous: { queue: Promise<Today> }) {
    // A reload replaces the promise; the failure it recovers from goes with it.
    if (previous.queue !== this.props.queue && this.state.error) {
      this.setState({ error: null })
    }
  }

  render() {
    const { error } = this.state
    if (!error) return this.props.children
    if (error instanceof Unauthorized) {
      return <SignIn onSignedIn={this.props.reload} />
    }
    return (
      <Shell>
        <p className="text-bad">{error.message}</p>
        <button type="button" className="self-start text-sm underline" onClick={this.props.reload}>
          Try again
        </button>
      </Shell>
    )
  }
}

/** The frame every screen sits in: one column, comfortable on a phone. */
export function Shell({ children }: { children: ReactNode }) {
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col gap-6 px-5 py-8 text-text">
      <header className="flex items-baseline justify-between">
        <h1 className="text-xl font-semibold tracking-tight">hilvan</h1>
        <span className="font-mono text-2xs text-faint">v{__APP_VERSION__}</span>
      </header>
      {children}
    </main>
  )
}
