import { use, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'

import { api, speechUrl, type Checked, type Formula, type FormulaDue, type Rating, type Stitch, type Turn } from '@/api'
import { answerHeard, answerSaid, tookSince, type Outcome } from '@/answers'
import { Shell } from '@/App'
import { checker, Unchecked } from '@/core'
import { Hold } from '@/Hold'
import { RATING_KEYS, useKeys, type Key } from '@/keys'
import {
  ask,
  formLabels,
  gradeLine,
  instruction,
  keptLine,
  ladder,
  nextPrompt,
  paceLine,
  ratings,
  seconds,
  stitchLine,
  tabs,
  tally,
  verdictLine,
  whenBack,
  type Prompt,
} from '@/prompts'
import { TURNS } from '@/sitting'
import { Speak } from '@/Speak'
import { player, sides, tempoFor, urlOf, useRecorder, useStopOnLeave, usePlaying } from '@/speech'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/** A typed turn and what the check made of it. */
interface Typed {
  turn: Turn
  checked: Checked
}

type Phase =
  | { kind: 'asking'; turn: number; prompt: Prompt; revealed: boolean }
  | { kind: 'typing'; turn: number; prompt: Prompt; text: string; checked: Checked | null; failed: string | null }
  | { kind: 'laddering' }
  | { kind: 'choosing' }
  | { kind: 'grading' }
  | { kind: 'graded'; rating: Rating; outcome: Outcome }

/**
 * One formula, drilled.
 *
 * Said, the loop is: read a prompt in your own language, type the sentence,
 * and the checker says whether it is right - and when it is not, whether the
 * shape slipped, a word did not come, or the words came in the wrong order.
 * The checker is the server's own code, built to run here (`core.ts`), so the
 * verdict comes on a train as it does at home; the turns are the answer, and
 * the tutor grades them again when they arrive. Understood, the loop is
 * heard: the English is played, the learner recalls what it means, looks,
 * and grades it themselves - what was understood is not something to type.
 *
 * The questions were drawn when Today was read (`sitting.ts`), so their
 * sounds are on the device. A form switched to draws its own.
 *
 * The caller mounts this with the card as `key`, so moving to the next
 * formula is a fresh component rather than an effect that resets four
 * pieces of state and has to be kept in step with them.
 */
export function Drill({
  due,
  prompts,
  position,
  total,
  onAnswered,
  onLeave,
}: {
  due: FormulaDue
  prompts: readonly Prompt[]
  position: number
  total: number
  onAnswered: () => void
  onLeave: () => void
}) {
  const checking = use(checker())
  // The formula being drilled. It starts as the one the queue offered and
  // becomes whichever sister the learner switches to: what is on screen is
  // what gets graded, always.
  const [shown, setShown] = useState<Formula>(due.formula)
  // Where the form on screen stands: what decides how fast it is said.
  const [shownStitch, setShownStitch] = useState<Stitch>(due.stitch)
  const [switching, setSwitching] = useState(false)
  useStopOnLeave()

  const saying = due.direction === 'produce'
  /** The question of a turn: the one planned, for the queue's own form. */
  const promptFor = (formula: Formula, turn: number): Prompt => {
    const planned = formula.id === due.formula.id ? prompts[turn] : undefined
    return planned ?? nextPrompt(formula, turn, Math.random)
  }
  const fresh = (formula: Formula, turn: number): Phase => {
    const prompt = promptFor(formula, turn)
    return saying ? typing(turn, prompt) : { kind: 'asking', turn, prompt, revealed: false }
  }
  // The first question is settled once, when the drill is built: drawing it
  // during render would hand the learner a different sentence on every
  // repaint.
  const [phase, setPhase] = useState<Phase>(() => fresh(due.formula, 0))
  // The turns typed for the form on screen, in order: what the grade is.
  const [typed, setTyped] = useState<Typed[]>([])
  const [explaining, setExplaining] = useState(due.is_new)
  const startedAt = useRef<number | null>(null)
  startedAt.current ??= Date.now()

  const rungs = ladder(shown)
  const tempo = tempoFor(shownStitch, due.is_new && shown.id === due.formula.id)

  const next = (turn: number) => setPhase(fresh(shown, turn))

  const switchTo = (id: string) => {
    if (id === shown.id) return
    const sister = shown.sisters.find((candidate) => candidate.id === id)
    setSwitching(true)
    void api
      .formula(id)
      .then((formula) => {
        setShown(formula)
        setShownStitch(sister?.stitch ?? 'new')
        setPhase(fresh(formula, 0))
        // Turns typed for the form before it are not this form's answer.
        setTyped([])
        // The clock starts over: how long this form took must not include the
        // time spent on the one before it.
        startedAt.current = Date.now()
      })
      // A switch that fails - out of reach, the form never kept - leaves the
      // drill where it was, which is a working drill: the learner loses a
      // look at another form, not their sitting.
      .catch(() => undefined)
      .finally(() => setSwitching(false))
  }

  const took = () => tookSince(startedAt.current)

  /** The answer, into the line on this device; the tutor's reply when it comes in time. */
  const give = (rating: Rating, send: () => Promise<Outcome>) => {
    setPhase({ kind: 'grading' })
    void send()
      .then((outcome) => setPhase({ kind: 'graded', rating, outcome }))
      // The answer could not even be kept here: the sitting goes on, and the
      // card comes back with the next read.
      .catch(() => onAnswered())
  }

  // `shown`, not the queue's formula: the learner grades what they were just
  // asked, which is the form on screen.
  const gradeHeard = (rating: Rating) => give(rating, () => answerHeard(shown.id, rating, took()))

  const gradeSaid = (easy: boolean) => {
    const rating = checking.rate(
      typed.map((entry) => entry.checked.failure),
      easy,
    )
    give(rating, () =>
      answerSaid(
        shown.id,
        typed.map((entry) => entry.turn),
        easy,
        took(),
      ),
    )
  }

  // Done typing: a flawless round asks the one thing the machine cannot see,
  // anything else is graded as it stands.
  const finishSaid = () => {
    if (typed.length > 0 && typed.every((entry) => entry.checked.correct)) setPhase({ kind: 'choosing' })
    else gradeSaid(false)
  }
  const finish = () => (saying ? finishSaid() : setPhase({ kind: 'choosing' }))

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

  const check = (event?: FormEvent) => {
    event?.preventDefault()
    if (phase.kind !== 'typing' || phase.checked) return
    const turn: Turn = { asked: phase.prompt.asked, typed: phase.text }
    let checked: Checked
    try {
      checked = checking.check(shown, turn)
    } catch (error) {
      setPhase({ ...phase, failed: error instanceof Unchecked ? error.message : 'The answer could not be checked.' })
      return
    }
    setTyped((before) => [...before, { turn, checked }])
    setPhase({ ...phase, checked, failed: null })
    // The field is done with: on a phone that puts the keyboard away so the
    // verdict is in view, and its focus ring is not read as a mark.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
    if (answerUrl) void player.play(answerUrl)
  }

  const lastTurn = phase.kind === 'asking' || phase.kind === 'typing' ? phase.turn + 1 >= TURNS : false
  const turnDone = (phase.kind === 'typing' && phase.checked !== null) || (phase.kind === 'asking' && phase.revealed)
  const advance = () => {
    if (phase.kind !== 'asking' && phase.kind !== 'typing') return
    if (lastTurn) finish()
    else next(phase.turn + 1)
  }

  // The keyboard: the space bar does what the screen is waiting for, the
  // numbers grade.
  const keys: Partial<Record<Key, () => void>> = {}
  if (explaining) keys.space = () => setExplaining(false)
  else if (phase.kind === 'asking' && !phase.revealed) keys.space = reveal
  else if (turnDone) keys.space = advance
  else if (phase.kind === 'laddering') keys.space = finish
  else if (phase.kind === 'graded') keys.space = onAnswered
  else if (phase.kind === 'choosing' && !saying) {
    for (const [key, rating] of Object.entries(RATING_KEYS)) keys[key as Key] = () => gradeHeard(rating)
  } else if (phase.kind === 'choosing' && saying) {
    keys['3'] = () => gradeSaid(false)
    keys['4'] = () => gradeSaid(true)
  }
  useKeys(keys)

  const big = 'px-4 py-4 text-xl'
  let actions: ReactNode = null
  if (explaining) {
    actions = (
      <Button onClick={() => setExplaining(false)} variant="primary" size={null} className={big}>
        Got it
      </Button>
    )
  } else if (phase.kind === 'typing' && phase.checked === null) {
    actions = (
      <Button type="submit" form="turn" variant="primary" size={null} className={big}>
        {phase.text.trim() === '' ? "I don't know" : 'Check'}
      </Button>
    )
  } else if (phase.kind === 'asking' && !phase.revealed) {
    actions = (
      <Button onClick={reveal} variant="primary" size={null} className={big}>
        {instruction(due.direction)}
      </Button>
    )
  } else if (turnDone) {
    actions = (
      <>
        <div className="flex items-baseline justify-between gap-3">
          {rungs.length > 1 ? (
            <Button onClick={() => setPhase({ kind: 'laddering' })} variant="link" className="text-base text-faint underline">
              Run the ladder
            </Button>
          ) : (
            <span />
          )}
          {!lastTurn && (
            <Button onClick={finish} variant="link" className="text-base text-faint underline">
              Grade it now
            </Button>
          )}
        </div>
        <Button onClick={advance} variant="primary" size={null} className={big}>
          {lastTurn ? (saying ? 'Grade it' : 'How did it go?') : 'Next'}
        </Button>
      </>
    )
  } else if (phase.kind === 'laddering') {
    actions = (
      <Button onClick={finish} variant="primary" size={null} className={big}>
        {saying ? 'Done - grade it' : 'Done - how did it go?'}
      </Button>
    )
  } else if (phase.kind === 'choosing' && saying) {
    actions = (
      <div className="grid grid-cols-2 gap-2">
        <Grade onClick={() => gradeSaid(false)} label="Good" hint="it came" shortcut="3" />
        <Grade onClick={() => gradeSaid(true)} label="Easy" hint="without thinking" shortcut="4" />
      </div>
    )
  } else if (phase.kind === 'choosing') {
    actions = (
      <div className="grid grid-cols-2 gap-2">
        {ratings.map(({ rating, label, hint }, index) => (
          <Grade key={rating} onClick={() => gradeHeard(rating)} label={label} hint={hint} shortcut={String(index + 1)} />
        ))}
      </div>
    )
  } else if (phase.kind === 'graded') {
    actions = (
      <Button onClick={onAnswered} variant="primary" size={null} className={big}>
        {position < total ? 'Next' : 'Finish'}
      </Button>
    )
  }

  return (
    <Shell
      actions={actions}
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Stop for now
        </Button>
      }
    >
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
          {shown.parents.length > 0 && <p className="text-base text-faint">Built on {shown.parents.map((parent) => parent.name).join(' · ')}</p>}
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
        </section>
      )}

      {!explaining && phase.kind === 'typing' && question && (
        <section className="flex flex-col gap-6">
          <div className="flex items-start justify-between gap-3">
            <p className="text-2xl leading-snug">{question.native}</p>
            <Speak url={questionUrl} label="Hear the question" />
          </div>

          <form id="turn" onSubmit={check} className="flex flex-col gap-3">
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
            {phase.failed && <p className="text-base text-bad">{phase.failed}</p>}
          </form>

          {phase.checked && (
            <>
              <Verdict checked={phase.checked} answerUrl={answerUrl} />
              {answerUrl && <OwnVoice key={answerUrl} native={answerUrl} />}
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

          {phase.revealed && (
            <div className="flex items-start justify-between gap-3 border-t border-line pt-4">
              <p className="text-2xl leading-snug text-accent">{question.target}</p>
              <Speak url={answerUrl} label="Hear the answer" />
            </div>
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
        </section>
      )}

      {!explaining && phase.kind === 'choosing' && (
        <p className="text-xl text-dim">{saying ? `${tally(typed.map((entry) => entry.checked))} Did it cost thought?` : 'How did that go?'}</p>
      )}

      {phase.kind === 'grading' && <p className="text-xl text-dim">Saving…</p>}

      {phase.kind === 'graded' && (
        <section className="flex flex-col gap-4">
          {saying && <p className="text-xl">{gradeLine(phase.rating, phase.outcome.kind === 'taken' ? phase.outcome.reviewed.words : [])}</p>}
          {phase.outcome.kind === 'taken' ? (
            <>
              <p className="text-xl">
                {stitchLine(phase.outcome.reviewed.stitch)} Back {whenBack(phase.outcome.reviewed.interval_days)}.
              </p>
              <Hold stability={phase.outcome.reviewed.stability} was={phase.outcome.reviewed.stability_before} />
              {paceLine(phase.outcome.reviewed.pace) && <p className="text-base text-faint">{paceLine(phase.outcome.reviewed.pace)}</p>}
            </>
          ) : (
            <p className="text-base text-faint">{keptLine()}</p>
          )}
        </section>
      )}
    </Shell>
  )
}

/** A fresh typing turn. */
function typing(turn: number, prompt: Prompt): Phase {
  return { kind: 'typing', turn, prompt, text: '', checked: null, failed: null }
}

/** One grade, with what it means and the key that gives it. */
export function Grade({ onClick, label, hint, shortcut }: { onClick: () => void; label: string; hint: string; shortcut: string }) {
  return (
    <Button onClick={onClick} variant="ghost" size={null} className="flex-col items-start gap-0.5 px-4 py-3 text-left text-text">
      <span className="flex w-full items-baseline justify-between gap-2">
        <span className="text-xl font-medium">{label}</span>
        {/* The key, for a keyboard; a phone has none, and the hint reads on. */}
        <kbd className="hidden font-mono text-xs text-faint pointer-fine:inline">{shortcut}</kbd>
      </span>
      <span className="text-base text-faint">{hint}</span>
    </Button>
  )
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
function Forms({ formula, busy, onSwitch }: { formula: Formula; busy: boolean; onSwitch: (id: string) => void }) {
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
