import { Component, use, useCallback, useEffect, useState, Suspense, type ReactNode } from 'react'

import {
  api,
  Unauthorized,
  type Drift,
  type Due,
  type Heard,
  type InventoryRun,
  type OwnWord,
  type Read,
  type Standings,
  type VoicesScreen as Voices,
} from '@/api'
import { BreakScreen } from '@/BreakScreen'
import { checker } from '@/core'
import { countWaiting, deliveryState, drain } from '@/delivery'
import { DeviceScreen } from '@/DeviceScreen'
import { Drill } from '@/Drill'
import { DriftScreen } from '@/DriftScreen'
import { InventoryScreen } from '@/InventoryScreen'
import { ListenScreen } from '@/ListenScreen'
import { Mark } from '@/Mark'
import { holdReads, holdShell, holdSounds, setBadge } from '@/offline'
import { held, settle } from '@/outbox'
import { OwnWordsScreen } from '@/OwnWordsScreen'
import { SignIn } from '@/SignIn'
import { badgeCount, cardOf, fitting, plan, soundsOf, unanswered, type Plan } from '@/sitting'
import { opening as thenOpening } from '@/then'
import { ThenAndNowScreen } from '@/ThenAndNowScreen'
import { TodayScreen } from '@/TodayScreen'
import { noteLine } from '@/schedule'
import { VoicesScreen } from '@/VoicesScreen'
import { WordCard } from '@/WordCard'
import { Button } from '@/components/ui/button'
import { Toast, ToastAction, ToastClose, ToastDescription, ToastProvider, ToastTitle, ToastViewport, useToastManager } from '@/components/ui/toast'

/**
 * Today as this device holds it: the read, and its queue less the cards
 * answered here since - on their way to the tutor, or landed after the read
 * - with the questions each formula will ask.
 */
export interface Day {
  read: Read
  queue: Due[]
  plan: Plan
}

/** Where the planned questions are kept between reads (see `sitting.ts`). */
const PLAN_KEY = 'hilvan.plan'

function keptPlan(): Plan {
  try {
    return JSON.parse(localStorage.getItem(PLAN_KEY) ?? '{}') as Plan
  } catch {
    return {}
  }
}

function keepPlan(planned: Plan): void {
  try {
    localStorage.setItem(PLAN_KEY, JSON.stringify(planned))
  } catch {
    // Not kept: the next read draws the questions again, and their sounds
    // are fetched again while in reach.
  }
}

/** Forgets the planned questions: the device is signed out. */
export function forgetPlan(): void {
  try {
    localStorage.removeItem(PLAN_KEY)
  } catch {
    // Nothing kept to forget.
  }
}

/** Reads Today and lays the answers given on this device over it. */
async function loadDay(): Promise<Day> {
  // What waits goes first, without holding the read up: an answer that lands
  // after the read began is taken off the queue here all the same.
  void drain()
  const read = await api.today()
  if (!read.cached) await settle(read.readAt)
  const answered = new Set((await held()).map((sending) => sending.card))
  const queue = unanswered(read.today.queue, answered)
  const planned = plan(queue, keptPlan(), Math.random)
  keepPlan(planned)
  return { read, queue, plan: planned }
}

/**
 * The minutes the app was opened for - "Ten minutes" from the home screen
 * shortcut - or `null` for the plain app.
 */
function minutesAsked(): number | null {
  const asked = Number(new URLSearchParams(window.location.search).get('minutes'))
  return Number.isInteger(asked) && asked > 0 && asked <= 120 ? asked : null
}

/**
 * What is on screen once the day has been read. The screens that read
 * something of their own carry the promise of it, made when the learner
 * asked to go there - React unwraps it with `use`, as it does the day.
 */
type View =
  | { kind: 'today' }
  | { kind: 'drilling'; items: Due[]; index: number }
  | { kind: 'listening'; sentences: Promise<Heard[]> }
  | { kind: 'voices'; screen: Promise<Voices> }
  | { kind: 'own-words'; words: Promise<OwnWord[]> }
  | { kind: 'inventory'; run: Promise<InventoryRun> }
  | { kind: 'then'; standings: Promise<Standings> }
  | { kind: 'break' }
  | { kind: 'drift'; drift: Promise<Drift> }
  | { kind: 'device' }

/**
 * The tutor.
 *
 * One loop and the screens around it: what is waiting today, the drill and
 * the word card that work through it, and the door when there is a password
 * on it.
 */
export function App() {
  // The day is a promise held in state rather than a fetch inside an effect:
  // React unwraps it with `use`, Suspense covers the wait and the boundary
  // below covers the failure, so there is no loading flag and no error flag
  // to keep in step with each other.
  const [day, setDay] = useState(loadDay)
  // A reload also starts the sitting over, back on Today: a screen whose own
  // read failed would otherwise come straight back with the same failure.
  const [generation, setGeneration] = useState(0)
  const reload = useCallback(() => {
    setDay(loadDay())
    setGeneration((current) => current + 1)
  }, [])
  // Read once: the shortcut opens the first sitting, not every one after.
  const [asked] = useState(minutesAsked)
  useEffect(() => {
    if (asked !== null) window.history.replaceState(null, '', '/')
  }, [asked])
  useDeliveryWhenever()

  return (
    <ToastProvider>
      <Boundary reload={reload} day={day}>
        <Suspense fallback={<Shell>Opening the tutor…</Shell>}>
          <Sitting key={generation} day={day} minutes={generation === 0 ? asked : null} reload={reload} />
        </Suspense>
      </Boundary>
      <ToastViewport>
        <Toasts />
      </ToastViewport>
    </ToastProvider>
  )
}

/**
 * Delivers what is waiting whenever there is reason to think the tutor can
 * be reached: the browser finding a connection, the app coming back to the
 * front, and a minute passing with something still waiting.
 */
function useDeliveryWhenever() {
  useEffect(() => {
    const tryNow = () => void drain()
    const onVisible = () => {
      if (document.visibilityState === 'visible') tryNow()
    }
    window.addEventListener('online', tryNow)
    document.addEventListener('visibilitychange', onVisible)
    const ticker = window.setInterval(() => {
      if (deliveryState().waiting > 0) tryNow()
    }, 60_000)
    void countWaiting()
    return () => {
      window.removeEventListener('online', tryNow)
      document.removeEventListener('visibilitychange', onVisible)
      window.clearInterval(ticker)
    }
  }, [])
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

/** Whether the app has been kept for the road in this page: once per load. */
let shellHeld = false

function Sitting({ day: dayPromise, minutes, reload }: { day: Promise<Day>; minutes: number | null; reload: () => void }) {
  const day = use(dayPromise)
  const { read, queue } = day
  const today = read.today
  const [view, setView] = useState<View>(() =>
    minutes !== null && queue.length > 0 ? { kind: 'drilling', items: fitting(queue, minutes, today.usual), index: 0 } : { kind: 'today' },
  )
  const toasts = useToastManager()

  // The checker is fetched while the tutor is in reach, so the first turn of
  // a sitting on a train does not wait for it.
  useEffect(() => {
    void checker().catch(() => undefined)
  }, [])

  // Kept for the road while the tutor is in reach: the app, the sounds of
  // this sitting, the forms a formula can switch to. Background work, one
  // request at a time.
  useEffect(() => {
    const now = Date.now()
    void setBadge(badgeCount(queue, [], now), today.coming, now)
    if (read.cached) return
    void (async () => {
      if (!shellHeld) {
        shellHeld = true
        await holdShell()
      }
      await holdSounds(soundsOf(queue, day.plan))
      const sisters = queue.flatMap((due) => (due.kind === 'formula' ? due.formula.sisters.map((sister) => sister.id) : []))
      await holdReads([...new Set(sisters)].map((id) => `/api/formulas/${encodeURIComponent(id)}`))
    })()
  }, [day, queue, read.cached, today.coming])

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
        <VoicesScreen screen={view.screen} onChanged={() => setView({ kind: 'voices', screen: api.voices() })} onLeave={backToToday} />
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

  if (view.kind === 'device') {
    return <DeviceScreen day={day} onLeave={backToToday} onSignedOut={reload} />
  }

  if (view.kind === 'drilling') {
    const due = view.items[view.index]
    if (!due) {
      return <Shell>Saving…</Shell>
    }
    const last = view.index + 1 >= view.items.length
    const turn = {
      position: view.index + 1,
      total: view.items.length,
      // The sitting ends on its last card: the day is read again, so Today
      // shows the tutor's counts rather than a guess made here.
      onAnswered: () => (last ? backToToday() : setView({ ...view, index: view.index + 1 })),
      onLeave: backToToday,
    }
    // A new item is a new card, not the same one reset: the key does what an
    // effect full of setState would otherwise have to. The direction is part
    // of a formula's, because the same formula asked backwards is a
    // different question and must not inherit the state of the one before.
    return due.kind === 'formula' ? (
      <Suspense fallback={<Shell>Opening the checker…</Shell>}>
        <Drill key={cardOf(due)} due={due} prompts={day.plan[cardOf(due)] ?? []} {...turn} />
      </Suspense>
    ) : (
      <WordCard key={cardOf(due)} due={due} {...turn} />
    )
  }

  return (
    <TodayScreen
      day={day}
      onStart={(items) => setView({ kind: 'drilling', items, index: 0 })}
      onListen={() => setView({ kind: 'listening', sentences: api.listen() })}
      onVoices={() => setView({ kind: 'voices', screen: api.voices() })}
      onOwnWords={() => setView({ kind: 'own-words', words: api.ownWords() })}
      onInventory={() => setView({ kind: 'inventory', run: api.inventory() })}
      onThen={() => setView({ kind: 'then', standings: thenOpening(api.standing, new Date()) })}
      onTakeBreak={() => setView({ kind: 'break' })}
      onBack={(id) => void api.endBreak(id).then(reload)}
      onCountAway={countAway}
      onDrift={() => setView({ kind: 'drift', drift: api.drift() })}
      onDevice={() => setView({ kind: 'device' })}
    />
  )
}

/**
 * Where a failed request lands.
 *
 * Being turned away is not an error to show the learner - it is the login
 * screen. Anything else is said plainly, with the one button that can help.
 */
class Boundary extends Component<{ children: ReactNode; day: Promise<Day>; reload: () => void }, { error: Error | null }> {
  state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error : new Error(String(error)) }
  }

  componentDidUpdate(previous: { day: Promise<Day> }) {
    // A reload replaces the promise; the failure it recovers from goes with it.
    if (previous.day !== this.props.day && this.state.error) {
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

/**
 * The frame every screen sits in: one column, comfortable on a phone.
 *
 * Built for one hand: the screen's own actions - check, next, the grades -
 * stand in `actions`, held at the bottom of the screen where a thumb reaches,
 * however long the card above them is. `aside` sits in the header, across
 * from the name: the way out of a card, which is pressed once a sitting and
 * belongs out of the thumb's way.
 */
export function Shell({ children, actions, aside }: { children: ReactNode; actions?: ReactNode; aside?: ReactNode }) {
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col px-5 pt-6 text-text">
      {/* The mark and the name, the way every app in the line wears them. */}
      <header className="flex items-center justify-between gap-3">
        <h1 className="flex items-center gap-2 font-mono text-lg font-semibold tracking-tight">
          <Mark />
          hilvan
        </h1>
        {aside ?? <span className="font-mono text-xs text-faint">v{__APP_VERSION__}</span>}
      </header>
      <div className="flex flex-1 flex-col gap-6 py-6">{children}</div>
      {actions && (
        <div className="sticky bottom-0 -mx-5 flex flex-col gap-2 border-t border-line bg-bg px-5 pt-3 pb-[max(1rem,env(safe-area-inset-bottom))]">
          {actions}
        </div>
      )}
    </main>
  )
}
