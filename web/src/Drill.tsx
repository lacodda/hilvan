import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'

import { api, speechUrl, type Checked, type Formula, type FormulaDue, type Rating, type Reviewed, type Stitch, type Turn } from '@/api'
import { Shell } from '@/App'
import { Hold } from '@/Hold'
import { ask, formLabels, gradeLine, instruction, ladder, nextPrompt, paceLine, ratings, seconds, stitchLine, tabs, tally, verdictLine, whenBack, type Prompt } from '@/prompts'
import { Speak } from '@/Speak'
import { player, sides, tempoFor, urlOf, useRecorder, useStopOnLeave, usePlaying } from '@/speech'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/** How many turns one formula gets before it is graded. */
const TURNS = 4

/** A typed turn and what the check made of it. */
interface Typed {
  turn: Turn
  checked: Checked
}

type Phase =
  | { kind: 'asking'; turn: number; prompt: Prompt; revealed: boolean }
  | { kind: 'typing'; turn: number; prompt: Prompt; text: string; checking: boolean; checked: Checked | null; failed: boolean }
  | { kind: 'laddering' }
  | { kind: 'choosing' }
  | { kind: 'grading' }
  | { kind: 'graded'; reviewed: Reviewed }

/**
 * One formula, drilled.
 *
 * Said, the loop is: read a prompt in your own language, type the sentence,
 * and the machine says whether it is right - and when it is not, whether the
 * shape slipped, a word did not come, or the words came in the wrong order.
 * The turns are the answer: the server grades them, and the learner only
 * adds whether a flawless round cost thought. Understood, the loop is heard:
 * the English is played, the learner recalls what it means, looks, and
 * grades it themselves - what was understood is not something to type.
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
  due: FormulaDue
  position: number
  total: number
  onAnswered: () => void
  onLeave: () => void
}) {
  // The formula being drilled. It starts as the one the queue offered and
  // becomes whichever sister the learner switches to: what is on screen is
  // what gets graded, always.
  const [shown, setShown] = useState<Formula>(due.formula)
  // Where the form on screen stands: what decides how fast it is said.
  const [shownStitch, setShownStitch] = useState<Stitch>(due.stitch)
  const [switching, setSwitching] = useState(false)
  useStopOnLeave()

  const saying = due.direction === 'produce'
  const first = (formula: Formula): Phase => {
    const prompt = nextPrompt(formula, 0, Math.random)
    return saying ? typing(0, prompt) : { kind: 'asking', turn: 0, prompt, revealed: false }
  }
  // The first prompt is drawn once, when the drill is built: drawing it
  // during render would hand the learner a different sentence on every
  // repaint.
  const [phase, setPhase] = useState<Phase>(() => first(due.formula))
  // The turns typed for the form on screen, in order: what the grade is.
  const [typed, setTyped] = useState<Typed[]>([])
  const [explaining, setExplaining] = useState(due.is_new)
  const startedAt = useRef<number | null>(null)
  startedAt.current ??= Date.now()

  const rungs = ladder(shown)
  const tempo = tempoFor(shownStitch, due.is_new && shown.id === due.formula.id)

  const next = (turn: number) => {
    const prompt = nextPrompt(shown, turn, Math.random)
    setPhase(saying ? typing(turn, prompt) : { kind: 'asking', turn, prompt, revealed: false })
  }

  const switchTo = (id: string) => {
    if (id === shown.id) return
    const sister = shown.sisters.find((candidate) => candidate.id === id)
    setSwitching(true)
    void api
      .formula(id)
      .then((formula) => {
        setShown(formula)
        setShownStitch(sister?.stitch ?? 'new')
        setPhase(first(formula))
        // Turns typed for the form before it are not this form's answer.
        setTyped([])
        // The clock starts over: how long this form took must not include the
        // time spent on the one before it.
        startedAt.current = Date.now()
      })
      // A switch that fails leaves the drill where it was, which is a working
      // drill: the learner loses a look at another form, not their sitting.
      .catch(() => undefined)
      .finally(() => setSwitching(false))
  }

  const took = () => (startedAt.current === null ? null : Date.now() - startedAt.current)

  const gradeHeard = useCallback(
    (rating: Rating) => {
      setPhase({ kind: 'grading' })
      void api
        // `shown`, not the queue's formula: the learner grades what they were
        // just asked, which is the form on screen.
        .reviewHeard(shown.id, rating, startedAt.current === null ? null : Date.now() - startedAt.current)
        .then((reviewed) => setPhase({ kind: 'graded', reviewed }))
        // A failed save is not worth stranding the learner mid-sitting: the
        // queue is re-read from the server on the way out anyway.
        .catch(() => onAnswered())
    },
    [shown.id, onAnswered],
  )

  const gradeSaid = (easy: boolean) => {
    setPhase({ kind: 'grading' })
    void api
      .reviewSaid(
        shown.id,
        typed.map((entry) => entry.turn),
        easy,
        took(),
      )
      .then((reviewed) => setPhase({ kind: 'graded', reviewed }))
      .catch(() => onAnswered())
  }

  // Done typing: a flawless round asks the one thing the machine cannot see,
  // anything else is graded as it stands.
  const finishSaid = () => {
    if (typed.length > 0 && typed.every((entry) => entry.checked.correct)) setPhase({ kind: 'choosing' })
    else gradeSaid(false)
  }

  const prompt = phase.kind === 'asking' || phase.kind === 'typing' ? phase.prompt : null
  const question = prompt ? ask(prompt, due.direction) : null
  const said = prompt ? sides(prompt, due.direction, shown) : null
  const questionUrl = said ? urlOf(said.question, tempo) : null
  const answerUrl = said ? urlOf(said.answer, tempo) : null

  // Asked backwards, the question is the language being learnt, and it is
  // heard before it is read: understanding is a listening skill first.
  const listening = !saying && !explaining && phase.kind === 'asking' && !phase.revealed
  useEffect(() => {
    if (listening && questionUrl) void player.play(questionUrl)
  }, [listening, questionUrl])

  const reveal = () => {
    if (phase.kind !== 'asking') return
    setPhase({ ...phase, revealed: true })
    // The answer is said the moment it is shown: hearing the right sentence
    // straight after your own is the whole of the correction.
    if (answerUrl) void player.play(answerUrl)
  }

  const check = (event: FormEvent) => {
    event.preventDefault()
    if (phase.kind !== 'typing' || phase.checking || phase.checked) return
    const turn: Turn = { asked: phase.prompt.asked, typed: phase.text }
    setPhase({ ...phase, checking: true, failed: false })
    void api
      .check(shown.id, turn)
      .then((checked) => {
        setTyped((before) => [...before, { turn, checked }])
        setPhase({ ...phase, checking: false, checked })
        // The field is done with: on a phone that puts the keyboard away so
        // the verdict is in view, and its focus ring is not read as a mark.
        if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
        if (answerUrl) void player.play(answerUrl)
      })
      .catch(() => setPhase({ ...phase, checking: false, failed: true }))
  }

  const lastTurn = phase.kind === 'asking' || phase.kind === 'typing' ? phase.turn + 1 >= TURNS : false
  const afterTurn = (
    <div className="flex flex-col gap-2">
      {!lastTurn && phase.kind !== 'laddering' ? (
        <Button
          onClick={() => (phase.kind === 'asking' || phase.kind === 'typing') && next(phase.turn + 1)}
          variant="primary"
          size={null}
          className="px-4 py-4 text-xl"
        >
          Next
        </Button>
      ) : (
        <Button
          onClick={() => (saying ? finishSaid() : setPhase({ kind: 'choosing' }))}
          variant="primary"
          size={null}
          className="px-4 py-4 text-xl"
        >
          {saying ? 'Grade it' : 'How did it go?'}
        </Button>
      )}
      <div className="flex items-baseline justify-between gap-3">
        {rungs.length > 1 ? (
          <Button onClick={() => setPhase({ kind: 'laddering' })} variant="link" className="text-base text-faint underline">
            Run the ladder
          </Button>
        ) : (
          <span />
        )}
        {!lastTurn && (
          <Button
            onClick={() => (saying ? finishSaid() : setPhase({ kind: 'choosing' }))}
            variant="link"
            className="text-base text-faint underline"
          >
            Grade it now
          </Button>
        )}
      </div>
    </div>
  )

  return (
    <Shell>
      <header className="flex items-baseline justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 className="text-xl font-semibold tracking-tight">{shown.name}</h2>
          <p className="font-mono text-base text-dim">{shown.pattern}</p>
        </div>
        <span className="shrink-0 font-mono text-xs text-faint">
          {position}/{total}
        </span>
      </header>

      <Forms formula={shown} busy={switching} onSwitch={switchTo} />

      {/* How firmly the card holds, before it is asked. Only the queue's own
          form: a sister switched to is another card, held its own way. */}
      {!due.is_new && shown.id === due.formula.id && phase.kind !== 'graded' && <Hold stability={due.stability} />}

      {!saying && <p className="text-xs tracking-caption text-faint uppercase">Understanding - what does it mean?</p>}

      {explaining && (
        <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
          {shown.parents.length > 0 && (
            <p className="text-base text-faint">Built on {shown.parents.map((parent) => parent.name).join(' · ')}</p>
          )}
          <div className="flex items-start gap-3">
            <p className="flex-1 text-lg leading-relaxed text-dim">{shown.explanation}</p>
            <Speak url={speechUrl(shown.languages.native, shown.explanation, 'normal')} label="Read the explanation aloud" />
          </div>
          <ul className="flex flex-col gap-2">
            {shown.samples.slice(0, 3).map((sample) => (
              <li key={sample.target} className="flex items-center gap-3 text-lg leading-relaxed">
                <span className="flex-1">
                  <span className="text-dim">{sample.native}</span> <span className="text-text">{sample.target}</span>
                </span>
                <Speak url={speechUrl(shown.languages.target, sample.target, tempo)} label={`Hear "${sample.target}"`} />
              </li>
            ))}
          </ul>
          <Button onClick={() => setExplaining(false)} variant="primary" size={null} className="self-start px-4 py-2 text-lg">
            Got it
          </Button>
        </section>
      )}

      {!explaining && phase.kind === 'typing' && question && (
        <section className="flex flex-col gap-6">
          <div className="flex items-start justify-between gap-3">
            <p className="text-2xl leading-snug">{question.native}</p>
            <Speak url={questionUrl} label="Hear the question" />
          </div>

          <form onSubmit={check} className="flex flex-col gap-3">
            <Input
              // A new turn is a new field: the text of the one before must not
              // linger, and the keyboard should come straight back up.
              key={phase.turn}
              autoFocus
              value={phase.text}
              onChange={(event) => setPhase({ ...phase, text: event.target.value })}
              readOnly={phase.checked !== null}
              aria-label={instruction('produce')}
              placeholder={instruction('produce')}
              lang={shown.languages.target}
              // The tutor checks the spelling; the phone must not fix it first.
              spellCheck={false}
              autoCorrect="off"
              autoComplete="off"
              enterKeyHint="done"
              aria-invalid={phase.checked !== null && !phase.checked.correct ? true : undefined}
              // text-lg: a phone zooms into a field whose text is under 16px.
              className="h-auto px-3 py-3 text-lg"
            />
            {phase.checked === null && (
              <Button type="submit" disabled={phase.checking} variant="primary" size={null} className="px-4 py-4 text-xl">
                {phase.checking ? 'Checking…' : phase.text.trim() === '' ? "I don't know" : 'Check'}
              </Button>
            )}
            {phase.failed && <p className="text-base text-bad">The answer could not be checked. Try again.</p>}
          </form>

          {phase.checked && (
            <>
              <Verdict checked={phase.checked} answerUrl={answerUrl} />
              {answerUrl && <OwnVoice key={answerUrl} native={answerUrl} />}
              {afterTurn}
            </>
          )}

          <p className="text-xs text-faint">
            Turn {phase.turn + 1} of {TURNS}
            {due.pace && <> · usually {seconds(due.pace.typical_ms)}</>}
          </p>
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
              {afterTurn}
            </>
          ) : (
            <Button onClick={reveal} variant="ghost" size={null} className="px-4 py-4 text-xl text-text">
              {instruction(due.direction)}
            </Button>
          )}

          <p className="text-xs text-faint">
            Turn {phase.turn + 1} of {TURNS}
            {due.pace && <> · usually {seconds(due.pace.typical_ms)}</>}
          </p>
        </section>
      )}

      {!explaining && phase.kind === 'laddering' && (
        <section className="flex flex-col gap-4">
          <p className="text-xl text-dim">Every person, straight through. Say them out loud without stopping.</p>
          <ul className="flex flex-col gap-1">
            {rungs.map((rung) => (
              <li key={rung.target} className="flex items-center gap-3 rounded-md bg-raise px-3 py-2">
                <span className="w-16 shrink-0 text-base text-faint">{rung.native}</span>
                <span className="flex-1 text-xl">{rung.target}</span>
                <Speak url={speechUrl(shown.languages.target, rung.target, tempo)} label={`Hear "${rung.target}"`} />
              </li>
            ))}
          </ul>
          <Button
            onClick={() => void player.playAll(rungs.map((rung) => speechUrl(shown.languages.target, rung.target, tempo)))}
            variant="link"
            className="self-start text-base text-dim underline"
          >
            Hear them all
          </Button>
          <Button
            onClick={() => (saying ? finishSaid() : setPhase({ kind: 'choosing' }))}
            variant="primary"
            size={null}
            className="px-4 py-4 text-xl"
          >
            {saying ? 'Done - grade it' : 'Done - how did it go?'}
          </Button>
        </section>
      )}

      {!explaining && phase.kind === 'choosing' && saying && (
        <section className="flex flex-col gap-3">
          <p className="text-xl">{tally(typed.map((entry) => entry.checked))} Did it cost thought?</p>
          <Button onClick={() => gradeSaid(false)} variant="ghost" size={null} className="items-baseline justify-between px-4 py-4 text-left text-text">
            <span className="text-xl font-medium">Good</span>
            <span className="text-base text-faint">it came</span>
          </Button>
          <Button onClick={() => gradeSaid(true)} variant="ghost" size={null} className="items-baseline justify-between px-4 py-4 text-left text-text">
            <span className="text-xl font-medium">Easy</span>
            <span className="text-base text-faint">without thinking</span>
          </Button>
        </section>
      )}

      {!explaining && phase.kind === 'choosing' && !saying && (
        <section className="flex flex-col gap-3">
          <p className="text-xl text-dim">How did that go?</p>
          {ratings.map(({ rating, label, hint }) => (
            <Button
              key={rating}
              onClick={() => gradeHeard(rating)}
              variant="ghost"
              size={null}
              className="items-baseline justify-between px-4 py-4 text-left text-text"
            >
              <span className="text-xl font-medium">{label}</span>
              <span className="text-base text-faint">{hint}</span>
            </Button>
          ))}
        </section>
      )}

      {phase.kind === 'grading' && <p className="text-xl text-dim">Saving…</p>}

      {phase.kind === 'graded' && (
        <section className="flex flex-col gap-4">
          {saying && <p className="text-xl">{gradeLine(phase.reviewed.rating, phase.reviewed.words)}</p>}
          <p className="text-xl">
            {stitchLine(phase.reviewed.stitch)} Back {whenBack(phase.reviewed.interval_days)}.
          </p>
          <Hold stability={phase.reviewed.stability} was={phase.reviewed.stability_before} />
          {paceLine(phase.reviewed.pace) && <p className="text-base text-faint">{paceLine(phase.reviewed.pace)}</p>}
          <Button onClick={onAnswered} variant="primary" size={null} className="px-4 py-4 text-xl">
            {position < total ? 'Next' : 'Finish'}
          </Button>
        </section>
      )}

      <footer className="mt-auto pt-6">
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Stop for now
        </Button>
      </footer>
    </Shell>
  )
}

/** A fresh typing turn. */
function typing(turn: number, prompt: Prompt): Phase {
  return { kind: 'typing', turn, prompt, text: '', checking: false, checked: null, failed: false }
}

/**
 * What the check found: whether it was right, and when it was not, the
 * right sentence with what did not come marked - the shape's own words in
 * one colour, the words it carries in another - and what was typed that the
 * sentence does not have.
 */
function Verdict({ checked, answerUrl }: { checked: Checked; answerUrl: string | null }) {
  return (
    <div className="flex flex-col gap-2 border-t border-line pt-4">
      <p className={`text-lg font-medium ${checked.correct ? 'text-good' : 'text-bad'}`}>{verdictLine(checked)}</p>
      <div className="flex items-start justify-between gap-3">
        <p className="text-2xl leading-snug">
          {checked.expected.map((word, index) => (
            <span key={index}>
              {index > 0 && ' '}
              <span
                className={
                  word.came
                    ? checked.correct
                      ? 'text-good'
                      : 'text-text'
                    : `rounded-sm px-0.5 underline decoration-2 underline-offset-4 ${word.word ? 'text-warn decoration-warn' : 'text-bad decoration-bad'}`
                }
              >
                {word.text}
              </span>
            </span>
          ))}
          {/[.?!]$/.exec(checked.closest)?.[0]}
        </p>
        <Speak url={answerUrl} label="Hear the answer" />
      </div>
      {checked.extra.length > 0 && (
        <p className="text-base text-faint">
          Not in it: <span className="text-dim line-through">{checked.extra.join(' ')}</span>
        </p>
      )}
      {checked.closest !== checked.answer && <p className="text-base text-faint">Also right: {checked.answer}</p>}
    </div>
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
    return <p className="text-sm text-faint">Recording your voice needs the https:// address of the tutor.</p>
  }

  const button = 'px-3 py-2 text-base'
  return (
    <div className="flex flex-wrap items-center gap-2">
      {recorder.state === 'recording' ? (
        <Button onClick={recorder.stop} variant="ghost" size={null} className={`${button} border-accent text-accent`}>
          Stop recording
        </Button>
      ) : (
        <Button onClick={recorder.start} variant="ghost" size={null} className={button}>
          {recorder.state === 'recorded' ? 'Record again' : 'Say it and record'}
        </Button>
      )}
      {recorder.state === 'recorded' && recorder.url && (
        <Button
          disabled={playing !== null}
          onClick={() => recorder.url && void player.playAll([recorder.url, native], 600)}
          variant="ghost"
          size={null}
          className={button}
        >
          You, then the voice
        </Button>
      )}
      {recorder.state === 'refused' && <span className="text-sm text-faint">The microphone was not allowed.</span>}
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
          <Button
            key={tab.id}
            // The current form is not disabled but inert: a disabled tab
            // would fade, and the one being drilled is the one to look at.
            disabled={busy}
            onClick={() => onSwitch(tab.id)}
            aria-current={current ? 'true' : undefined}
            variant={null}
            size={null}
            className={`flex-1 flex-col gap-1 rounded-md px-2 py-2 text-base ${current ? 'bg-accent text-on-accent' : 'bg-raise text-dim'}`}
          >
            <span className="font-medium">{formLabels[tab.form]}</span>
            <span className={`h-1 w-6 rounded-full ${stitchBar(tab.stitch, current)}`} />
          </Button>
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
