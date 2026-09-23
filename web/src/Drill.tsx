import { useCallback, useEffect, useRef, useState } from 'react'

import { api, speechUrl, type Due, type Formula, type Rating, type Reviewed, type Stitch } from '@/api'
import { Shell } from '@/App'
import { ask, formLabels, instruction, ladder, nextPrompt, paceLine, ratings, seconds, tabs, whenBack, type Prompt } from '@/prompts'
import { Speak } from '@/Speak'
import { player, sides, tempoFor, urlOf, useRecorder, useStopOnLeave, usePlaying } from '@/speech'

/** How many turns one formula gets before the learner grades it. */
const TURNS = 4

type Phase =
  | { kind: 'asking'; turn: number; prompt: Prompt; revealed: boolean }
  | { kind: 'laddering' }
  | { kind: 'grading' }
  | { kind: 'graded'; reviewed: Reviewed }

/**
 * One formula, drilled.
 *
 * The loop is the whole method: read a prompt, say the answer out loud, then
 * look. Nothing is typed - typing is a different skill, and it turns a
 * ten-second turn into a minute.
 *
 * The caller mounts this with the formula's id as `key`, so moving to the
 * next formula is a fresh component rather than an effect that resets four
 * pieces of state and has to be kept in step with them.
 */
export function Drill({
  due,
  position,
  total,
  onAnswered,
  onLeave,
}: {
  due: Due
  position: number
  total: number
  onAnswered: () => void
  onLeave: () => void
}) {
  // The formula being drilled. It starts as the one the queue offered and
  // becomes whichever sister the learner switches to: what is on screen is
  // what gets graded, always. A card that showed one form and graded another
  // would need a sentence of explanation, and a drill that needs explaining
  // on every turn is a drill with a design problem.
  const [shown, setShown] = useState<Formula>(due.formula)
  // Where the form on screen stands: what decides how fast it is said. The
  // queue's own card knows; a sister switched to brings its own.
  const [shownStitch, setShownStitch] = useState<Stitch>(due.stitch)
  const [switching, setSwitching] = useState(false)
  useStopOnLeave()

  // The first prompt is drawn once, when the drill is built: drawing it
  // during render would hand the learner a different sentence on every
  // repaint.
  const [phase, setPhase] = useState<Phase>(() => ({
    kind: 'asking',
    turn: 0,
    prompt: nextPrompt(due.formula, 0, Math.random),
    revealed: false,
  }))
  const [explaining, setExplaining] = useState(due.is_new)
  const startedAt = useRef<number | null>(null)
  startedAt.current ??= Date.now()

  const rungs = ladder(shown)
  const tempo = tempoFor(shownStitch, due.is_new && shown.id === due.formula.id)

  const ask_ = (turn: number) =>
    setPhase({ kind: 'asking', turn, prompt: nextPrompt(shown, turn, Math.random), revealed: false })

  const switchTo = (id: string) => {
    if (id === shown.id) return
    const sister = shown.sisters.find((candidate) => candidate.id === id)
    setSwitching(true)
    void api
      .formula(id)
      .then((formula) => {
        setShown(formula)
        setShownStitch(sister?.stitch ?? 'new')
        setPhase({ kind: 'asking', turn: 0, prompt: nextPrompt(formula, 0, Math.random), revealed: false })
        // The clock starts over: how long this form took must not include the
        // time spent on the one before it.
        startedAt.current = Date.now()
      })
      // A switch that fails leaves the drill where it was, which is a working
      // drill: the learner loses a look at another form, not their sitting.
      .catch(() => undefined)
      .finally(() => setSwitching(false))
  }

  const grade = useCallback(
    (rating: Rating) => {
      setPhase({ kind: 'grading' })
      const took = startedAt.current === null ? null : Date.now() - startedAt.current
      void api
        // `shown`, not the queue's formula: the learner grades what they were
        // just asked, which is the form on screen.
        .review(shown.id, rating, due.direction, took)
        .then((reviewed) => setPhase({ kind: 'graded', reviewed }))
        // A failed save is not worth stranding the learner mid-sitting: the
        // queue is re-read from the server on the way out anyway.
        .catch(() => onAnswered())
    },
    [shown.id, due.direction, onAnswered],
  )

  const question = phase.kind === 'asking' ? ask(phase.prompt, due.direction) : null
  const said = phase.kind === 'asking' ? sides(phase.prompt, due.direction, shown) : null
  const questionUrl = said ? urlOf(said.question, tempo) : null
  const answerUrl = said ? urlOf(said.answer, tempo) : null

  // Asked backwards, the question is the language being learnt, and it is
  // heard before it is read: understanding is a listening skill first.
  const listening = due.direction === 'recognise' && !explaining && phase.kind === 'asking' && !phase.revealed
  useEffect(() => {
    if (listening && questionUrl) void player.play(questionUrl)
  }, [listening, questionUrl])

  const reveal = () => {
    if (phase.kind !== 'asking') return
    setPhase({ ...phase, revealed: true })
    // The answer is said the moment it is shown: hearing the right sentence
    // straight after saying your own is the whole of the correction.
    if (answerUrl) void player.play(answerUrl)
  }

  return (
    <Shell>
      <header className="flex items-baseline justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 className="text-xl font-semibold tracking-tight">{shown.name}</h2>
          <p className="font-mono text-[0.8125rem] text-dim">{shown.pattern}</p>
        </div>
        <span className="shrink-0 font-mono text-[0.6875rem] text-faint">
          {position}/{total}
        </span>
      </header>

      <Forms formula={shown} busy={switching} onSwitch={switchTo} />

      {due.direction === 'recognise' && (
        <p className="text-[0.6875rem] tracking-caption text-faint uppercase">Understanding - what does it mean?</p>
      )}

      {explaining && (
        <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
          <div className="flex items-start gap-3">
            <p className="flex-1 text-[0.9375rem] leading-relaxed text-dim">{shown.explanation}</p>
            <Speak url={speechUrl(shown.languages.native, shown.explanation, 'normal')} label="Read the explanation aloud" />
          </div>
          <ul className="flex flex-col gap-2">
            {shown.samples.slice(0, 3).map((sample) => (
              <li key={sample.target} className="flex items-center gap-3 text-[0.9375rem] leading-relaxed">
                <span className="flex-1">
                  <span className="text-dim">{sample.native}</span> <span className="text-text">{sample.target}</span>
                </span>
                <Speak url={speechUrl(shown.languages.target, sample.target, tempo)} label={`Hear "${sample.target}"`} />
              </li>
            ))}
          </ul>
          <button
            type="button"
            onClick={() => setExplaining(false)}
            className="self-start rounded-md bg-accent px-4 py-2 text-[0.9375rem] font-medium text-on-accent"
          >
            Got it
          </button>
        </section>
      )}

      {!explaining && phase.kind === 'asking' && question && (
        <section className="flex flex-col gap-6">
          <div className="flex items-start justify-between gap-3">
            <p className="text-2xl leading-snug">{question.native}</p>
            <Speak url={questionUrl} label="Hear the question" />
          </div>

          {phase.revealed ? (
            <>
              <div className="flex items-start justify-between gap-3 border-t border-line pt-4">
                <p className="text-2xl leading-snug text-accent">{question.target}</p>
                <Speak url={answerUrl} label="Hear the answer" />
              </div>
              {due.direction === 'produce' && answerUrl && <OwnVoice key={answerUrl} native={answerUrl} />}
              <div className="flex flex-col gap-2">
                {phase.turn + 1 < TURNS ? (
                  <button
                    type="button"
                    onClick={() => ask_(phase.turn + 1)}
                    className="rounded-md bg-accent px-4 py-4 text-[1.0625rem] font-medium text-on-accent"
                  >
                    Next
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => setPhase({ kind: 'grading' })}
                    className="rounded-md bg-accent px-4 py-4 text-[1.0625rem] font-medium text-on-accent"
                  >
                    How did it go?
                  </button>
                )}
                <div className="flex items-baseline justify-between gap-3">
                  {rungs.length > 1 ? (
                    <button
                      type="button"
                      onClick={() => setPhase({ kind: 'laddering' })}
                      className="text-[0.8125rem] text-faint underline"
                    >
                      Run the ladder
                    </button>
                  ) : (
                    <span />
                  )}
                  <button
                    type="button"
                    onClick={() => setPhase({ kind: 'grading' })}
                    className="text-[0.8125rem] text-faint underline"
                  >
                    Grade it now
                  </button>
                </div>
              </div>
            </>
          ) : (
            <button
              type="button"
              onClick={reveal}
              className="rounded-md border border-line px-4 py-4 text-[1.0625rem] font-medium"
            >
              {instruction(due.direction)}
            </button>
          )}

          <p className="text-[0.6875rem] text-faint">
            Turn {phase.turn + 1} of {TURNS}
            {due.pace && <> · usually {seconds(due.pace.typical_ms)}</>}
          </p>
        </section>
      )}

      {!explaining && phase.kind === 'laddering' && (
        <section className="flex flex-col gap-4">
          <p className="text-[1.0625rem] text-dim">Every person, straight through. Say them out loud without stopping.</p>
          <ul className="flex flex-col gap-1">
            {rungs.map((rung) => (
              <li key={rung.target} className="flex items-center gap-3 rounded-md bg-raise px-3 py-2">
                <span className="w-16 shrink-0 text-[0.8125rem] text-faint">{rung.native}</span>
                <span className="flex-1 text-[1.0625rem]">{rung.target}</span>
                <Speak url={speechUrl(shown.languages.target, rung.target, tempo)} label={`Hear "${rung.target}"`} />
              </li>
            ))}
          </ul>
          <button
            type="button"
            onClick={() => void player.playAll(rungs.map((rung) => speechUrl(shown.languages.target, rung.target, tempo)))}
            className="self-start text-[0.8125rem] text-dim underline"
          >
            Hear them all
          </button>
          <button
            type="button"
            onClick={() => setPhase({ kind: 'grading' })}
            className="rounded-md bg-accent px-4 py-4 text-[1.0625rem] font-medium text-on-accent"
          >
            Done - how did it go?
          </button>
        </section>
      )}

      {!explaining && phase.kind === 'grading' && (
        <section className="flex flex-col gap-3">
          <p className="text-[1.0625rem] text-dim">How did that go?</p>
          {ratings.map(({ rating, label, hint }) => (
            <button
              key={rating}
              type="button"
              onClick={() => grade(rating)}
              className="flex items-baseline justify-between rounded-md border border-line px-4 py-4 text-left"
            >
              <span className="text-[1.0625rem] font-medium">{label}</span>
              <span className="text-[0.8125rem] text-faint">{hint}</span>
            </button>
          ))}
        </section>
      )}

      {phase.kind === 'graded' && (
        <section className="flex flex-col gap-4">
          <p className="text-[1.0625rem]">
            {stitchLine(phase.reviewed.stitch)} Back {whenBack(phase.reviewed.interval_days)}.
          </p>
          {paceLine(phase.reviewed.pace) && <p className="text-[0.8125rem] text-faint">{paceLine(phase.reviewed.pace)}</p>}
          <button
            type="button"
            onClick={onAnswered}
            className="rounded-md bg-accent px-4 py-4 text-[1.0625rem] font-medium text-on-accent"
          >
            {position < total ? 'Next formula' : 'Finish'}
          </button>
        </section>
      )}

      <footer className="mt-auto pt-6">
        <button type="button" onClick={onLeave} className="text-[0.8125rem] text-faint underline">
          Stop for now
        </button>
      </footer>
    </Shell>
  )
}

/**
 * Your own voice next to the native one.
 *
 * Record the answer you just said, then hear yours and the sample one after
 * the other: half of what speech recognition would give, for nothing, and
 * with the learner's own ear as the judge. The recording lives in memory
 * until the card goes, and is never sent anywhere.
 */
function OwnVoice({ native }: { native: string }) {
  const recorder = useRecorder()
  const playing = usePlaying()

  if (!recorder.available) {
    return <p className="text-[0.75rem] text-faint">Recording your voice needs the https:// address of the tutor.</p>
  }

  const button = 'rounded-md border border-line px-3 py-2 text-[0.8125rem] font-medium'
  return (
    <div className="flex flex-wrap items-center gap-2">
      {recorder.state === 'recording' ? (
        <button type="button" onClick={recorder.stop} className={`${button} border-accent text-accent`}>
          Stop recording
        </button>
      ) : (
        <button type="button" onClick={recorder.start} className={button}>
          {recorder.state === 'recorded' ? 'Record again' : 'Record yourself'}
        </button>
      )}
      {recorder.state === 'recorded' && recorder.url && (
        <button
          type="button"
          disabled={playing !== null}
          onClick={() => recorder.url && void player.playAll([recorder.url, native], 600)}
          className={`${button} disabled:opacity-50`}
        >
          You, then the voice
        </button>
      )}
      {recorder.state === 'refused' && <span className="text-[0.75rem] text-faint">The microphone was not allowed.</span>}
    </div>
  )
}

/**
 * The switch between the three forms of one shape.
 *
 * Statement, negation, question on one card: a learner who can say "I am
 * tired" and cannot ask "Are you tired?" has half a formula, and the switch is
 * what makes the other halves one tap away. The dot on a tab says where that
 * form stands, so the untouched question is visible next to the sewn
 * statement.
 */
function Forms({
  formula,
  busy,
  onSwitch,
}: {
  formula: Formula
  busy: boolean
  onSwitch: (id: string) => void
}) {
  const switches = tabs(formula)
  if (switches.length === 0) return null

  return (
    <nav className="flex gap-1" aria-label="Forms of this shape">
      {switches.map((tab) => {
        const current = tab.current
        return (
          <button
            key={tab.id}
            type="button"
            disabled={busy || current}
            onClick={() => onSwitch(tab.id)}
            aria-current={current ? 'true' : undefined}
            className={`flex flex-1 flex-col items-center gap-1 rounded-md px-2 py-2 text-[0.8125rem] ${
              current ? 'bg-accent text-on-accent' : 'bg-raise text-dim'
            }`}
          >
            <span className="font-medium">{formLabels[tab.form]}</span>
            <span className={`h-1 w-6 rounded-full ${stitchBar(tab.stitch, current)}`} />
          </button>
        )
      })}
    </nav>
  )
}

/**
 * How far along a form is, as a bar rather than a number.
 *
 * The form being drilled has no bar of its own to show - the card it is being
 * graded on is the one on screen - so it reads as current instead.
 */
function stitchBar(stitch: Stitch | null, current: boolean): string {
  if (current || stitch === null) return 'bg-on-accent/40'
  switch (stitch) {
    case 'new':
      return 'bg-line'
    case 'basted':
      return 'bg-accent/50'
    case 'sewn':
      return 'bg-accent'
  }
}

function stitchLine(stitch: Reviewed['stitch']): string {
  switch (stitch) {
    case 'new':
      return 'Still new.'
    case 'basted':
      return 'Basted - tacked in place.'
    case 'sewn':
      return 'Sewn.'
  }
}
