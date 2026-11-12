import { useEffect, useRef, useState, type ReactNode } from 'react'

import { speechUrl, type Choice, type PairDue, type Question, type Side } from '@/api'
import { answerPair, tookSince, type Outcome } from '@/answers'
import { Shell } from '@/App'
import { Hold } from '@/Hold'
import { useKeys, type Key } from '@/keys'
import { Button } from '@/components/ui/button'
import { gap, swapLine } from '@/pairs'
import { keptLine, paceLine, stitchLine, whenBack } from '@/prompts'
import { Speak } from '@/Speak'
import { player, tempoFor, useStopOnLeave } from '@/speech'

type Phase =
  | { kind: 'meeting' }
  | { kind: 'asking'; index: number }
  | { kind: 'shown'; index: number }
  | { kind: 'grading' }
  | { kind: 'graded'; outcome: Outcome }

/**
 * Two words the learner puts one for the other, told apart.
 *
 * The pair came from the learner's own typing - one word where the other
 * stood, more than once - so a new pair opens on that: the two words, what
 * each means, which was typed for which. Then the sentences they were met
 * in, each with its word left out, and the two to choose from. A choice is
 * shown right or not at once, with the sentence whole and heard; the tutor
 * grades the round from the sentences themselves.
 *
 * Mounted with the pair's id as `key`, like the drill.
 */
export function PairCard({
  due,
  position,
  total,
  onAnswered,
  onLeave,
}: {
  due: PairDue
  position: number
  total: number
  onAnswered: () => void
  onLeave: () => void
}) {
  const { pair } = due
  const [phase, setPhase] = useState<Phase>(due.is_new ? { kind: 'meeting' } : { kind: 'asking', index: 0 })
  const [choices, setChoices] = useState<Choice[]>([])
  const startedAt = useRef<number | null>(null)
  useStopOnLeave()

  const tempo = tempoFor(due.stitch, due.is_new)
  const asking = phase.kind === 'asking' && phase.index === 0
  useEffect(() => {
    if (asking && startedAt.current === null) startedAt.current = Date.now()
  }, [asking])

  const question = phase.kind === 'asking' || phase.kind === 'shown' ? pair.questions[phase.index] : undefined
  const sentenceUrl = question ? speechUrl(pair.languages.target, question.text, tempo) : null

  const choose = (side: Side) => {
    if (phase.kind !== 'asking' || !question) return
    setChoices([...choices, { text: question.text, chosen: side.id }])
    setPhase({ kind: 'shown', index: phase.index })
    if (sentenceUrl) void player.play(sentenceUrl)
  }

  const next = () => {
    if (phase.kind !== 'shown') return
    if (phase.index + 1 < pair.questions.length) {
      setPhase({ kind: 'asking', index: phase.index + 1 })
      return
    }
    setPhase({ kind: 'grading' })
    void answerPair(pair.id, choices, tookSince(startedAt.current))
      .then((outcome) => setPhase({ kind: 'graded', outcome }))
      // The answer could not even be kept here: the sitting goes on, and the
      // card comes back with the next read.
      .catch(() => onAnswered())
  }

  const keys: Partial<Record<Key, () => void>> = {}
  if (phase.kind === 'meeting') keys.space = () => setPhase({ kind: 'asking', index: 0 })
  else if (phase.kind === 'asking') {
    pair.words.forEach((side, index) => {
      keys[String(index + 1) as Key] = () => choose(side)
    })
  } else if (phase.kind === 'shown') keys.space = next
  else if (phase.kind === 'graded') keys.space = onAnswered
  useKeys(keys)

  const big = 'px-4 py-4 text-xl'
  let actions: ReactNode = null
  if (phase.kind === 'meeting') {
    actions = (
      <Button onClick={() => setPhase({ kind: 'asking', index: 0 })} variant="primary" size={null} className={big}>
        Got it - ask me
      </Button>
    )
  } else if (phase.kind === 'asking') {
    actions = (
      <div className="grid grid-cols-2 gap-2">
        {pair.words.map((side, index) => (
          <Button key={side.id} onClick={() => choose(side)} variant="ghost" size={null} className="flex-col gap-0.5 px-4 py-4 text-text">
            <span className="flex w-full items-baseline justify-between gap-2">
              <span className="text-xl font-medium">{side.lemma}</span>
              <kbd className="hidden font-mono text-xs text-faint pointer-fine:inline">{index + 1}</kbd>
            </span>
          </Button>
        ))}
      </div>
    )
  } else if (phase.kind === 'shown') {
    const last = phase.index + 1 >= pair.questions.length
    actions = (
      <Button onClick={next} variant="primary" size={null} className={big}>
        {last ? 'Done' : 'Next sentence'}
      </Button>
    )
  } else if (phase.kind === 'graded') {
    actions = (
      <Button onClick={onAnswered} variant="primary" size={null} className={big}>
        {position < total ? 'Next' : 'Finish'}
      </Button>
    )
  }

  const chosen = phase.kind === 'shown' ? choices[phase.index] : undefined

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
        <p className="text-xs tracking-caption text-faint uppercase">{due.is_new ? 'Two words you mix up' : 'Which one?'}</p>
        <span className="shrink-0 font-mono text-xs text-faint">
          {position}/{total}
        </span>
      </header>

      {!due.is_new && phase.kind !== 'graded' && <Hold stability={due.stability} />}

      {(phase.kind === 'meeting' || phase.kind === 'graded') && <Words words={pair.words} />}
      {phase.kind === 'meeting' && swapLine(pair.words) && <p className="text-lg text-dim">{swapLine(pair.words)}</p>}

      {question && (
        <section className="flex flex-col gap-3">
          <p className="text-xs tracking-caption text-faint uppercase">
            Sentence {phase.kind === 'asking' || phase.kind === 'shown' ? phase.index + 1 : 0} of {pair.questions.length}
          </p>
          {chosen ? <Filled question={question} right={chosen.chosen === question.answer} url={sentenceUrl} /> : <Gapped question={question} />}
          {question.translation && <p className="text-lg text-dim">{question.translation}</p>}
          {chosen && chosen.chosen !== question.answer && (
            <p className="text-base text-bad">
              Here it is &ldquo;{lemmaOf(pair.words, question.answer)}&rdquo;, not &ldquo;{lemmaOf(pair.words, chosen.chosen)}&rdquo;.
            </p>
          )}
        </section>
      )}

      {phase.kind === 'grading' && <p className="text-xl text-dim">Saving…</p>}

      {phase.kind === 'graded' &&
        (phase.outcome.kind === 'taken' ? (
          <section className="flex flex-col gap-4">
            <p className="text-xl">
              {stitchLine(phase.outcome.reviewed.stitch)} Back {whenBack(phase.outcome.reviewed.interval_days)}.
            </p>
            <Hold stability={phase.outcome.reviewed.stability} was={phase.outcome.reviewed.stability_before} />
            {paceLine(phase.outcome.reviewed.pace) && <p className="text-base text-faint">{paceLine(phase.outcome.reviewed.pace)}</p>}
          </section>
        ) : (
          <p className="text-base text-faint">{keptLine()}</p>
        ))}
    </Shell>
  )
}

function lemmaOf(words: readonly Side[], id: string): string {
  return words.find((side) => side.id === id)?.lemma ?? id
}

/** The two words side by side, each with what it means. */
function Words({ words }: { words: readonly Side[] }) {
  return (
    <section className="grid grid-cols-2 gap-2">
      {words.map((side) => (
        <div key={side.id} className="flex flex-col gap-1 rounded-md bg-raise px-3 py-3">
          <span className="text-2xl font-semibold">{side.lemma}</span>
          {side.gloss && <span className="text-lg text-accent">{side.gloss}</span>}
        </div>
      ))}
    </section>
  )
}

/** The sentence with its word left out. */
function Gapped({ question }: { question: Question }) {
  const { before, after } = gap(question)
  return (
    <p className="text-2xl leading-snug">
      {before}
      <span className="mx-0.5 inline-block min-w-16 border-b-2 border-accent align-baseline" aria-label="the word left out">
        &nbsp;
      </span>
      {after}
    </p>
  )
}

/** The sentence whole again, its word marked right or not, and heard. */
function Filled({ question, right, url }: { question: Question; right: boolean; url: string | null }) {
  const { before, word, after } = gap(question)
  return (
    <div className="flex items-start justify-between gap-3">
      <p className="flex-1 text-2xl leading-snug">
        {before}
        <mark className={`rounded-xs px-0.5 ${right ? 'bg-good-soft text-good' : 'bg-bad-soft text-bad'}`}>{word}</mark>
        {after}
      </p>
      <Speak url={url} label="Hear the sentence" />
    </div>
  )
}
