import { Component, use, useCallback, useState, Suspense, type ReactNode } from 'react'

import { api, Unauthorized, type Heard, type Today, type VoicesScreen as Voices } from '@/api'
import { Drill } from '@/Drill'
import { ListenScreen } from '@/ListenScreen'
import { Mark } from '@/Mark'
import { SignIn } from '@/SignIn'
import { TodayScreen } from '@/TodayScreen'
import { VoicesScreen } from '@/VoicesScreen'

/**
 * What is on screen once the queue has been read. The screens that read
 * something of their own carry the promise of it, made when the learner
 * asked to go there - React unwraps it with `use`, as it does the queue.
 */
type View =
  | { kind: 'today' }
  | { kind: 'drilling'; index: number }
  | { kind: 'listening'; sentences: Promise<Heard[]> }
  | { kind: 'voices'; screen: Promise<Voices> }

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
  // A reload also starts the sitting over, back on Today: a screen whose own
  // read failed would otherwise come straight back with the same failure.
  const [generation, setGeneration] = useState(0)
  const reload = useCallback(() => {
    setQueue(api.today())
    setGeneration((current) => current + 1)
  }, [])

  return (
    <Boundary reload={reload} queue={queue}>
      <Suspense fallback={<Shell>Opening the tutor…</Shell>}>
        <Sitting key={generation} queue={queue} reload={reload} />
      </Suspense>
    </Boundary>
  )
}

function Sitting({ queue, reload }: { queue: Promise<Today>; reload: () => void }) {
  const [view, setView] = useState<View>({ kind: 'today' })
  const today = use(queue)

  const backToToday = () => {
    setView({ kind: 'today' })
    reload()
  }

  if (view.kind === 'listening') {
    return (
      <Suspense fallback={<Shell>Gathering sentences…</Shell>}>
        <ListenScreen sentences={view.sentences} onLeave={backToToday} />
      </Suspense>
    )
  }

  if (view.kind === 'voices') {
    return (
      <Suspense fallback={<Shell>Asking the voices…</Shell>}>
        <VoicesScreen
          screen={view.screen}
          onChanged={() => setView({ kind: 'voices', screen: api.voices() })}
          onLeave={backToToday}
        />
      </Suspense>
    )
  }

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
        onLeave={backToToday}
      />
    )
  }

  return (
    <TodayScreen
      today={today}
      onStart={() => setView({ kind: 'drilling', index: 0 })}
      onListen={() => setView({ kind: 'listening', sentences: api.listen() })}
      onVoices={() => setView({ kind: 'voices', screen: api.voices() })}
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
        <button type="button" className="self-start text-[0.9375rem] underline" onClick={this.props.reload}>
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
      {/* The mark and the name, the way every app in the line wears them. */}
      <header className="flex items-center justify-between">
        <h1 className="flex items-center gap-2 font-mono text-[0.9375rem] font-semibold tracking-tight">
          <Mark />
          hilvan
        </h1>
        <span className="font-mono text-[0.6875rem] text-faint">v{__APP_VERSION__}</span>
      </header>
      {children}
    </main>
  )
}
