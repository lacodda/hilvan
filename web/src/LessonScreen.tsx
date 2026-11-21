import { use, useEffect, useMemo, useRef, useState } from 'react'

import { api, Unreachable, type Lesson, type Lessons } from '@/api'
import { answerByEar } from '@/answers'
import { Shell } from '@/App'
import { clock, hear, keep, kept as keptOnDevice, listened, minutesLine, nextStart, previousStart, remember, stretchAt, type Kept, type Listened } from '@/lessons'
import { player } from '@/speech'

import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Segment, SegmentedControl } from '@/components/ui/segmented-control'
import { useToastManager } from '@/components/ui/toast'

/** What became of a lesson's grades: how many held and slipped, and whether any wait on the phone. */
interface Sent {
  held: number
  slipped: number
  kept: boolean
}

/** How often a lesson being made is asked about. */
const FOLLOW_MS = 2_000

/** The lesson screen as it opens: what the tutor says, or `null` out of its reach. */
export function openLessons(): Promise<Lessons | null> {
  return api.lessons().catch((error: unknown) => {
    if (error instanceof Unreachable) return null
    throw error
  })
}

/**
 * Today by ear: the day's cards as an audio lesson for a walk.
 *
 * The lesson is made on the stand - Pimsleur's way, a question, a pause to
 * answer aloud in, the answer, room to say it again - and taken onto the
 * phone whole, so it plays out of reach of the tutor. What was asked to the
 * end is graded afterwards, held or slipped, and goes to the schedule like
 * any answer.
 */
export function LessonScreen({ opening, onLeave }: { opening: Promise<Lessons | null>; onLeave: () => void }) {
  const told = use(opening)
  const [minutes, setMinutes] = useState(told?.minutes ?? 25)
  const [latest, setLatest] = useState<Lesson | null>(told?.lessons[0] ?? null)
  const [device, setDevice] = useState<Kept | null | undefined>(undefined)
  const [taking, setTaking] = useState(false)
  const [asking, setAsking] = useState(false)
  const [sent, setSent] = useState<Sent | null>(null)
  // Graded here, whether or not the device could write it down.
  const [graded, setGraded] = useState<number | null>(null)
  const toasts = useToastManager()

  useEffect(() => {
    player.stop()
    void keptOnDevice().then(setDevice)
  }, [])

  // A lesson being made is followed until it is ready, or fails.
  const building = latest?.state === 'building' ? latest.id : null
  useEffect(() => {
    if (building === null) return undefined
    const timer = window.setInterval(() => {
      void api
        .lesson(building)
        .then(setLatest)
        .catch(() => undefined)
    }, FOLLOW_MS)
    return () => window.clearInterval(timer)
  }, [building])

  const failed = (what: string) => (error: unknown) =>
    toasts.add({
      title: what,
      description: error instanceof Unreachable ? 'Out of the tutor’s reach - try again at home.' : error instanceof Error ? error.message : String(error),
      type: 'error',
    })

  const ask = () => {
    setAsking(true)
    setSent(null)
    void api
      .askLesson(minutes)
      .then(setLatest)
      .catch(failed('Not started'))
      .finally(() => setAsking(false))
  }

  const take = (lesson: Lesson) => {
    setTaking(true)
    void keep(lesson)
      .then(setDevice)
      .catch(failed('Not taken'))
      .finally(() => setTaking(false))
  }

  const lengths = told?.lengths ?? [15, 25, 40]
  // A lesson heard and graded here is done with: it is neither played nor
  // offered again, and the next one is made.
  const done = (id: number) => id === graded || listened(id).graded
  const playable = device && !done(device.lesson.id) ? device : null
  const newer = latest?.state === 'ready' && latest.id !== playable?.lesson.id && !done(latest.id) ? latest : null

  return (
    <Shell
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back to today
        </Button>
      }
    >
      <header className="flex flex-col gap-1">
        <h2 className="text-2xl font-semibold tracking-tight">By ear</h2>
        <p className="text-base text-faint">Today’s cards as a lesson for a walk: a question, a pause to answer aloud, the answer, and room to say it again.</p>
      </header>

      {sent && (
        <section className="flex flex-col gap-2 rounded-md bg-raise p-4">
          <p className="text-xl">
            {sent.held} held{sent.slipped > 0 ? `, ${sent.slipped} slipped` : ''}.
          </p>
          <p className="text-base text-dim">
            {sent.kept
              ? 'Kept on this phone until the tutor is in reach; then what slipped comes back soon, the rest when it is due.'
              : 'Gone to your schedule: what slipped comes back soon, the rest when it is due.'}
          </p>
        </section>
      )}

      {playable && (
        <Player
          key={playable.lesson.id}
          kept={playable}
          onGraded={(grades) => {
            setSent(grades)
            setGraded(playable.lesson.id)
          }}
        />
      )}

      {!playable && device !== undefined && (
        <Making
          told={told !== null}
          done={device !== null}
          latest={latest}
          newer={newer}
          minutes={minutes}
          lengths={lengths}
          asking={asking}
          taking={taking}
          onMinutes={setMinutes}
          onAsk={ask}
          onTake={take}
        />
      )}

      {playable && newer && (
        <p className="text-base text-faint">A newer lesson is ready on the tutor; it comes once this one is graded.</p>
      )}
    </Shell>
  )
}

/** Asking for a lesson, following it while it is made, and taking it onto the device. */
function Making({
  told,
  done,
  latest,
  newer,
  minutes,
  lengths,
  asking,
  taking,
  onMinutes,
  onAsk,
  onTake,
}: {
  told: boolean
  /** Whether the lesson on this phone has been heard and graded. */
  done: boolean
  latest: Lesson | null
  newer: Lesson | null
  minutes: number
  lengths: number[]
  asking: boolean
  taking: boolean
  onMinutes: (minutes: number) => void
  onAsk: () => void
  onTake: (lesson: Lesson) => void
}) {
  if (!told) {
    return (
      <p className="text-xl text-dim">
        {done
          ? 'Out of the tutor’s reach, and the lesson on this phone is done. Make the next one at home and take it with you.'
          : 'Out of the tutor’s reach, and no lesson is on this phone. Make one at home and take it with you.'}
      </p>
    )
  }
  if (latest?.state === 'building') {
    return (
      <section className="flex flex-col gap-3">
        <p className="text-xl">Making a {latest.minutes}-minute lesson…</p>
        <Progress
          value={latest.total === 0 ? undefined : latest.done}
          max={Math.max(latest.total, 1)}
          label="Lines said so far"
        />
        <p className="text-base text-faint">{latest.total === 0 ? 'Choosing the phrases.' : `${latest.done} of ${latest.total} lines said.`} New lines take a moment each; those heard before are instant.</p>
      </section>
    )
  }
  if (newer) {
    return (
      <section className="flex flex-col gap-3">
        <p className="text-xl">
          A {minutesLine(newer.length_ms ?? 0)} lesson of {newer.phrases.length} phrases is ready.
        </p>
        <Button onClick={() => onTake(newer)} disabled={taking} variant="primary" size={null} className="self-start px-4 py-3 text-lg">
          {taking ? 'Taking it…' : 'Take it with you'}
        </Button>
        <p className="text-base text-faint">It is kept on this phone and plays without the tutor.</p>
      </section>
    )
  }
  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <span id="lesson-length" className="text-base text-dim">
          How long
        </span>
        <SegmentedControl aria-labelledby="lesson-length" value={String(minutes)} onValueChange={(value) => onMinutes(Number(value))}>
          {lengths.map((length) => (
            <Segment key={length} value={String(length)}>
              {length} min
            </Segment>
          ))}
        </SegmentedControl>
      </div>
      {latest?.state === 'failed' && latest.error && <p className="text-lg text-bad">{latest.error}</p>}
      <Button onClick={onAsk} disabled={asking} variant="primary" size={null} className="self-start px-4 py-3 text-lg">
        {latest?.state === 'failed' ? 'Try again' : `Make a ${minutes}-minute lesson`}
      </Button>
      <p className="text-base text-faint">What is due comes first, then what is about to be forgotten. Pairs stay on the cards.</p>
    </section>
  )
}

/** The lesson on the device, played: where it is, what has been heard, and the grading after. */
function Player({ kept, onGraded }: { kept: Kept; onGraded: (sent: Sent) => void }) {
  const { lesson, audio } = kept
  const chapters = lesson.chapters
  const length = lesson.length_ms ?? 0
  const url = useMemo(() => URL.createObjectURL(audio), [audio])
  useEffect(() => () => URL.revokeObjectURL(url), [url])
  const element = useRef<HTMLAudioElement>(null)
  const [state, setState] = useState<Listened>(() => listened(lesson.id))
  const [playing, setPlaying] = useState(false)
  const [grading, setGrading] = useState(false)
  // Where the player last reported, to tell playing from a jump.
  const last = useRef(state.at)

  useEffect(() => remember(lesson.id, state), [lesson.id, state])

  const at = state.at
  const stretch = chapters[stretchAt(chapters, at)]
  const asked = Object.keys(state.heard).length

  const seek = (ms: number) => {
    const audio = element.current
    if (audio) audio.currentTime = ms / 1_000
  }
  const toggle = () => {
    const audio = element.current
    if (!audio) return
    if (audio.paused) void audio.play()
    else audio.pause()
  }

  // The lock screen's buttons skip by phrase, as the ones here do.
  useEffect(() => {
    const session = typeof navigator === 'undefined' ? undefined : navigator.mediaSession
    if (!session) return undefined
    if (typeof MediaMetadata !== 'undefined') {
      session.metadata = new MediaMetadata({ title: `Lesson · ${minutesLine(length)}`, artist: 'hilvan' })
    }
    const now = () => (element.current?.currentTime ?? 0) * 1_000
    const handlers: [MediaSessionAction, MediaSessionActionHandler][] = [
      ['play', () => void element.current?.play()],
      ['pause', () => element.current?.pause()],
      ['previoustrack', () => seek(previousStart(chapters, now()))],
      [
        'nexttrack',
        () => {
          const next = nextStart(chapters, now())
          if (next !== null) seek(next)
        },
      ],
    ]
    for (const [action, handler] of handlers) {
      try {
        session.setActionHandler(action, handler)
      } catch {
        // An action this browser does not offer.
      }
    }
    return () => {
      for (const [action] of handlers) {
        try {
          session.setActionHandler(action, null)
        } catch {
          // Nothing was set.
        }
      }
    }
  }, [chapters, length])

  if (grading) {
    return (
      <Grade
        lesson={lesson}
        state={state}
        onBack={() => setGrading(false)}
        onSent={(counts) => {
          setState((before) => ({ ...before, graded: true }))
          remember(lesson.id, { ...state, graded: true })
          onGraded(counts)
        }}
      />
    )
  }

  return (
    <section className="flex flex-col gap-5">
      <audio
        ref={element}
        src={url}
        preload="auto"
        onLoadedMetadata={() => {
          // Back where the learner stopped, a few seconds early so the
          // question is heard again.
          if (state.at > 0) seek(Math.max(0, state.at - 3_000))
        }}
        onTimeUpdate={(event) => {
          const ms = Math.floor(event.currentTarget.currentTime * 1_000)
          const from = last.current
          last.current = ms
          setState((before) => hear(before, chapters, from, ms, Date.now()))
        }}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => {
          setPlaying(false)
          setGrading(true)
        }}
      />

      <div className="flex flex-col gap-2">
        <Progress value={Math.min(at, length)} max={Math.max(length, 1)} label="How far into the lesson" />
        <div className="flex items-baseline justify-between font-mono text-sm text-faint">
          <span>{clock(at)}</span>
          <span>{clock(length)}</span>
        </div>
      </div>

      <p className="text-lg text-dim">
        Phrase {stretch ? stretch.phrase + 1 : 0} of {lesson.phrases.length}
        {stretch?.turn === 'heard' ? ' · new, listen first' : ''}
      </p>

      <div className="flex items-center gap-3">
        <Button onClick={() => seek(previousStart(chapters, at))} variant="ghost" size={null} className="px-3 py-3 text-base text-text">
          Back a phrase
        </Button>
        <Button onClick={toggle} variant="primary" size={null} className="flex-1 px-4 py-4 text-xl">
          {playing ? 'Pause' : at > 0 ? 'Go on' : 'Play'}
        </Button>
        <Button
          onClick={() => {
            const next = nextStart(chapters, at)
            if (next !== null) seek(next)
          }}
          variant="ghost"
          size={null}
          className="px-3 py-3 text-base text-text"
        >
          Next phrase
        </Button>
      </div>

      <footer className="flex flex-col gap-2 border-t border-line pt-4">
        <Button
          onClick={() => {
            element.current?.pause()
            setGrading(true)
          }}
          variant="ghost"
          size={null}
          className="self-start px-4 py-3 text-lg text-text"
        >
          Done listening
        </Button>
        <p className="text-base text-faint">{asked === 0 ? 'Nothing asked to the end yet.' : `${asked} of ${lesson.phrases.length} phrases asked so far; you say how they went at the end.`}</p>
      </footer>
    </section>
  )
}

/** How it went: every phrase asked to the end, held unless the learner says it slipped. */
function Grade({ lesson, state, onBack, onSent }: { lesson: Lesson; state: Listened; onBack: () => void; onSent: (sent: Sent) => void }) {
  const heard = Object.keys(state.heard)
    .map(Number)
    .sort((a, b) => a - b)
  const [slipped, setSlipped] = useState<ReadonlySet<number>>(new Set())
  const [sending, setSending] = useState(false)

  const send = () => {
    setSending(true)
    void Promise.all(heard.map((phrase) => answerByEar(lesson.phrases[phrase]!.card, !slipped.has(phrase), state.heard[phrase]!)))
      .then((outcomes) =>
        onSent({ held: heard.length - slipped.size, slipped: slipped.size, kept: outcomes.some((outcome) => outcome.kind === 'kept') }),
      )
      .finally(() => setSending(false))
  }

  if (heard.length === 0) {
    return (
      <section className="flex flex-col gap-3">
        <p className="text-xl text-dim">Nothing was asked to the end yet, so there is nothing to say how it went.</p>
        <Button onClick={onBack} variant="primary" size={null} className="self-start px-4 py-3 text-lg">
          Back to the lesson
        </Button>
      </section>
    )
  }

  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h3 className="text-xl font-semibold">How did it go?</h3>
        <p className="text-base text-faint">Tap a phrase that did not come in the pause. The rest count as held.</p>
      </div>
      <ol className="flex flex-col gap-2">
        {heard.map((phrase) => {
          const said = lesson.phrases[phrase]!
          const gone = slipped.has(phrase)
          return (
            <li key={phrase}>
              <Button
                onClick={() =>
                  setSlipped((before) => {
                    const next = new Set(before)
                    if (gone) next.delete(phrase)
                    else next.add(phrase)
                    return next
                  })
                }
                aria-pressed={gone}
                variant="ghost"
                size={null}
                className="flex w-full flex-col items-stretch gap-1 px-3 py-3 text-left font-normal whitespace-normal text-text aria-pressed:bg-raise"
              >
                <span className="flex items-baseline justify-between gap-3">
                  <span className="text-lg text-accent">{said.target}</span>
                  <span className={`shrink-0 text-base ${gone ? 'text-bad' : 'text-faint'}`}>{gone ? 'Slipped' : 'Held'}</span>
                </span>
                <span className="text-base text-dim">{said.native}</span>
              </Button>
            </li>
          )
        })}
      </ol>
      <div className="flex items-center gap-3">
        <Button onClick={send} disabled={sending} variant="primary" size={null} className="px-4 py-3 text-lg">
          {sending ? 'Sending…' : 'That is how it went'}
        </Button>
        <Button onClick={onBack} variant="link" className="text-base text-faint underline">
          Back to the lesson
        </Button>
      </div>
    </section>
  )
}
