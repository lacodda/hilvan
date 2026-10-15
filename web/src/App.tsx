import { Component, use, useCallback, useEffect, useState, Suspense, type ReactNode } from 'react'

import {
  api,
  Unauthorized,
  type Drift,
  type Heard,
  type InventoryRun,
  type OwnWord,
  type Standings,
  type Today,
  type VoicesScreen as Voices,
} from '@/api'
import { BreakScreen } from '@/BreakScreen'
import { Drill } from '@/Drill'
import { DriftScreen } from '@/DriftScreen'
import { InventoryScreen } from '@/InventoryScreen'
import { ListenScreen } from '@/ListenScreen'
import { Mark } from '@/Mark'
import { OwnWordsScreen } from '@/OwnWordsScreen'
import { SignIn } from '@/SignIn'
import { opening } from '@/then'
import { ThenAndNowScreen } from '@/ThenAndNowScreen'
import { TodayScreen } from '@/TodayScreen'
import { noteLine } from '@/schedule'
import { VoicesScreen } from '@/VoicesScreen'
import { WordCard } from '@/WordCard'
import { Button } from '@/components/ui/button'
import { Toast, ToastAction, ToastClose, ToastDescription, ToastProvider, ToastTitle, ToastViewport, useToastManager } from '@/components/ui/toast'

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
  | { kind: 'own-words'; words: Promise<OwnWord[]> }
  | { kind: 'inventory'; run: Promise<InventoryRun> }
  | { kind: 'then'; standings: Promise<Standings> }
  | { kind: 'break' }
  | { kind: 'drift'; drift: Promise<Drift> }

/**
 * The tutor.
 *
 * One loop and the screens around it: what is waiting today, the drill and
 * the word card that work through it, and the door when there is a password
 * on it.
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
    <ToastProvider>
      <Boundary reload={reload} queue={queue}>
        <Suspense fallback={<Shell>Opening the tutor…</Shell>}>
          <Sitting key={generation} queue={queue} reload={reload} />
        </Suspense>
      </Boundary>
      <ToastViewport>
        <Toasts />
      </ToastViewport>
    </ToastProvider>
  )
}

/** What the manager holds, each as a toast: a sentence, and the one thing to do about it. */
function Toasts() {
  const { toasts } = useToastManager()
  return (
    <>
      {toasts.map((toast) => (
        <Toast key={toast.id} toast={toast}>
          <ToastTitle />
          <ToastDescription className="pr-6 text-sm" />
          {toast.actionProps && <ToastAction />}
          <ToastClose aria-label="Dismiss" />
        </Toast>
      ))}
    </>
  )
}

/** The weeks whose note has been raised in this tab: once is enough, until the week's first answer closes it. */
const noted = new Set<string>()

function Sitting({ queue, reload }: { queue: Promise<Today>; reload: () => void }) {
  const [view, setView] = useState<View>({ kind: 'today' })
  const today = use(queue)
  const toasts = useToastManager()

  const backToToday = () => {
    setView({ kind: 'today' })
    reload()
  }

  // Last week's slips open the first sitting of a week, by themselves: a
  // note rather than a screen, because there is nothing to decide - what
  // slipped comes back sooner on its own.
  const note = today.last_week
  useEffect(() => {
    if (!note || noted.has(note.starts_on)) return
    noted.add(note.starts_on)
    toasts.add({
      title: 'Last week',
      description: noteLine(note),
      timeout: 12_000,
      actionProps: { children: 'See them', onClick: () => setView({ kind: 'drift', drift: api.drift() }) },
    })
  }, [note, toasts])

  const countAway = () => {
    void api
      .countAway()
      .then((taken) => {
        reload()
        toasts.add({
          title: 'Counted as a break',
          description: 'What came due while you were away comes back over the days ahead.',
          timeout: 8_000,
          actionProps: { children: 'Undo', onClick: () => void api.removeBreak(taken.id).then(reload) },
        })
      })
      .catch((error: unknown) => toasts.add({ title: 'Not counted', description: error instanceof Error ? error.message : String(error), type: 'error' }))
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

  if (view.kind === 'own-words') {
    return (
      <Suspense fallback={<Shell>Reading your words…</Shell>}>
        <OwnWordsScreen words={view.words} onLeave={backToToday} />
      </Suspense>
    )
  }

  if (view.kind === 'inventory') {
    return (
      <Suspense fallback={<Shell>Finding the commonest words…</Shell>}>
        <InventoryScreen run={view.run} onLeave={backToToday} />
      </Suspense>
    )
  }

  if (view.kind === 'then') {
    return (
      <Suspense fallback={<Shell>Looking back…</Shell>}>
        <ThenAndNowScreen first={view.standings} onLeave={backToToday} />
      </Suspense>
    )
  }

  if (view.kind === 'break') {
    return <BreakScreen onTaken={backToToday} onLeave={backToToday} />
  }

  if (view.kind === 'drift') {
    return (
      <Suspense fallback={<Shell>Looking at the week…</Shell>}>
        <DriftScreen drift={view.drift} onLeave={backToToday} />
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
    const turn = {
      position: view.index + 1,
      total: today.queue.length,
      onAnswered: () => setView({ kind: 'drilling', index: view.index + 1 }),
      onLeave: backToToday,
    }
    // A new item is a new card, not the same one reset: the key does what an
    // effect full of setState would otherwise have to. The direction is part
    // of a formula's, because the same formula asked backwards is a
    // different question and must not inherit the state of the one before.
    return due.kind === 'formula' ? (
      <Drill key={`formula:${due.formula.id}:${due.direction}`} due={due} {...turn} />
    ) : (
      <WordCard key={`word:${due.word.id}`} due={due} {...turn} />
    )
  }

  return (
    <TodayScreen
      today={today}
      onStart={() => setView({ kind: 'drilling', index: 0 })}
      onListen={() => setView({ kind: 'listening', sentences: api.listen() })}
      onVoices={() => setView({ kind: 'voices', screen: api.voices() })}
      onOwnWords={() => setView({ kind: 'own-words', words: api.ownWords() })}
      onInventory={() => setView({ kind: 'inventory', run: api.inventory() })}
      onThen={() => setView({ kind: 'then', standings: opening(api.standing, new Date()) })}
      onTakeBreak={() => setView({ kind: 'break' })}
      onBack={(id) => void api.endBreak(id).then(reload)}
      onCountAway={countAway}
      onDrift={() => setView({ kind: 'drift', drift: api.drift() })}
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
        <Button variant="link" className="self-start text-lg text-text underline" onClick={this.props.reload}>
          Try again
        </Button>
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
        <h1 className="flex items-center gap-2 font-mono text-lg font-semibold tracking-tight">
          <Mark />
          hilvan
        </h1>
        <span className="font-mono text-xs text-faint">v{__APP_VERSION__}</span>
      </header>
      {children}
    </main>
  )
}
