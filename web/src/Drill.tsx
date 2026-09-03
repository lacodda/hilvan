import { useCallback, useRef, useState } from 'react'

import { api, type Due, type Rating, type Reviewed } from '@/api'
import { Shell } from '@/App'
import { nextPrompt, ratings, whenBack, type Prompt } from '@/prompts'

/** How many turns one formula gets before the learner grades it. */
const TURNS = 4

type Phase =
  | { kind: 'asking'; turn: number; prompt: Prompt; revealed: boolean }
  | { kind: 'grading' }
  | { kind: 'graded'; reviewed: Reviewed }

/**
 * One formula, drilled.
 *
 * The loop is the whole method: read a prompt in your own language, say the
 * English out loud, then look. Nothing is typed - typing is a different skill,
 * and it turns a ten-second turn into a minute.
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

  const ask = (turn: number) =>
    setPhase({ kind: 'asking', turn, prompt: nextPrompt(due.formula, turn, Math.random), revealed: false })

  const grade = useCallback(
    (rating: Rating) => {
    setPhase({ kind: 'grading' })
    const took = startedAt.current === null ? null : Date.now() - startedAt.current
    void api
      .review(due.formula.id, rating, took)
      .then((reviewed) => setPhase({ kind: 'graded', reviewed }))
      // A failed save is not worth stranding the learner mid-sitting: the
      // queue is re-read from the server on the way out anyway.
      .catch(() => onAnswered())
    },
    [due.formula.id, onAnswered],
  )

  return (
    <Shell>
      <header className="flex items-baseline justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">{due.formula.name}</h2>
          <p className="font-mono text-xs text-dim">{due.formula.pattern}</p>
        </div>
        <span className="shrink-0 font-mono text-2xs text-faint">
          {position}/{total}
        </span>
      </header>

      {explaining && (
        <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
          <p className="text-sm leading-relaxed text-dim">{due.formula.explanation}</p>
          <ul className="flex flex-col gap-1">
            {due.formula.samples.slice(0, 3).map((sample) => (
              <li key={sample.target} className="text-sm">
                <span className="text-dim">{sample.native}</span> <span className="text-text">{sample.target}</span>
              </li>
            ))}
          </ul>
          <button
            type="button"
            onClick={() => setExplaining(false)}
            className="self-start rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent"
          >
            Got it
          </button>
        </section>
      )}

      {!explaining && phase.kind === 'asking' && (
        <section className="flex flex-col gap-6">
          <p className="text-2xl leading-snug">{phase.prompt.native}</p>

          {phase.revealed ? (
            <>
              <p className="border-t border-line pt-4 text-2xl leading-snug text-accent">{phase.prompt.target}</p>
              <div className="flex flex-col gap-2">
                {phase.turn + 1 < TURNS ? (
                  <button
                    type="button"
                    onClick={() => ask(phase.turn + 1)}
                    className="rounded-md bg-accent px-4 py-4 text-base font-medium text-on-accent"
                  >
                    Next
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => setPhase({ kind: 'grading' })}
                    className="rounded-md bg-accent px-4 py-4 text-base font-medium text-on-accent"
                  >
                    How did it go?
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => setPhase({ kind: 'grading' })}
                  className="text-xs text-faint underline"
                >
                  Grade it now
                </button>
              </div>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setPhase({ ...phase, revealed: true })}
              className="rounded-md border border-line px-4 py-4 text-base font-medium"
            >
              Say it, then look
            </button>
          )}

          <p className="text-2xs text-faint">
            Turn {phase.turn + 1} of {TURNS}
          </p>
        </section>
      )}

      {!explaining && phase.kind === 'grading' && (
        <section className="flex flex-col gap-3">
          <p className="text-base text-dim">How did that go?</p>
          {ratings.map(({ rating, label, hint }) => (
            <button
              key={rating}
              type="button"
              onClick={() => grade(rating)}
              className="flex items-baseline justify-between rounded-md border border-line px-4 py-4 text-left"
            >
              <span className="text-base font-medium">{label}</span>
              <span className="text-xs text-faint">{hint}</span>
            </button>
          ))}
        </section>
      )}

      {phase.kind === 'graded' && (
        <section className="flex flex-col gap-4">
          <p className="text-base">
            {stitchLine(phase.reviewed.stitch)} Back {whenBack(phase.reviewed.interval_days)}.
          </p>
          <button
            type="button"
            onClick={onAnswered}
            className="rounded-md bg-accent px-4 py-4 text-base font-medium text-on-accent"
          >
            {position < total ? 'Next formula' : 'Finish'}
          </button>
        </section>
      )}

      <footer className="mt-auto pt-6">
        <button type="button" onClick={onLeave} className="text-xs text-faint underline">
          Stop for now
        </button>
      </footer>
    </Shell>
  )
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
