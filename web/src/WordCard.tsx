import { useCallback, useEffect, useRef, useState } from 'react'

import { api, speechUrl, type Context, type Rating, type Reviewed, type WordDue } from '@/api'
import { Shell } from '@/App'
import { Button } from '@/components/ui/button'
import { paceLine, ratings, seconds, stitchLine, whenBack } from '@/prompts'
import { Speak } from '@/Speak'
import { player, tempoFor, useStopOnLeave } from '@/speech'
import { commonness, mark } from '@/words'

type Phase =
  | { kind: 'meeting' }
  | { kind: 'asking' }
  | { kind: 'revealed' }
  | { kind: 'grading' }
  | { kind: 'graded'; reviewed: Reviewed }

/**
 * One word, asked in its sentence.
 *
 * A word is never asked alone: the sentence it is heard in - its anchor - is
 * played first and shown with the word marked, and the learner recalls what
 * the word means there. Then the word itself, slowly, with its transcription
 * and its meaning, and the other sentences it has been met in.
 *
 * A word seen for the first time is met before it is asked: everything is
 * on the card, and only then is it taken away to be recalled.
 *
 * Mounted with the word's id as `key`, like the drill.
 */
export function WordCard({
  due,
  position,
  total,
  onAnswered,
  onLeave,
}: {
  due: WordDue
  position: number
  total: number
  onAnswered: () => void
  onLeave: () => void
}) {
  const { word } = due
  const [phase, setPhase] = useState<Phase>(due.is_new ? { kind: 'meeting' } : { kind: 'asking' })
  const startedAt = useRef<number | null>(null)
  useStopOnLeave()

  const anchor = word.contexts[0]
  const others = word.contexts.slice(1)
  const tempo = tempoFor(due.stitch, due.is_new)
  const sentenceUrl = anchor ? speechUrl(word.languages.target, anchor.text, tempo) : null
  // The word on its own is always said slowly: it is heard at speed in the
  // sentence, and on its own it is there to have every sound in it heard.
  const wordUrl = speechUrl(word.languages.target, word.lemma, 'slow')

  // Heard before it is read: understanding a word is a listening skill first.
  const asking = phase.kind === 'asking'
  useEffect(() => {
    if (!asking) return
    startedAt.current = Date.now()
    if (sentenceUrl) void player.play(sentenceUrl)
  }, [asking, sentenceUrl])

  // The first meeting says the word, then its sentence. A card leaves the
  // meeting once and never comes back to it, so this plays once.
  const meeting = phase.kind === 'meeting'
  useEffect(() => {
    if (meeting) void player.playAll([wordUrl, ...(sentenceUrl ? [sentenceUrl] : [])], 500)
  }, [meeting, wordUrl, sentenceUrl])

  const reveal = () => {
    setPhase({ kind: 'revealed' })
    void player.play(wordUrl)
  }

  const grade = useCallback(
    (rating: Rating) => {
      setPhase({ kind: 'grading' })
      const took = startedAt.current === null ? null : Date.now() - startedAt.current
      void api
        .reviewWord(word.id, rating, took)
        .then((reviewed) => setPhase({ kind: 'graded', reviewed }))
        // A failed save leaves the sitting going; the queue is re-read from
        // the server on the way out.
        .catch(() => onAnswered())
    },
    [word.id, onAnswered],
  )

  const shown = phase.kind === 'meeting' || phase.kind === 'revealed'

  return (
    <Shell>
      <header className="flex items-baseline justify-between gap-3">
        <p className="text-xs tracking-caption text-faint uppercase">{due.is_new ? 'A new word' : 'A word in its sentence'}</p>
        <span className="shrink-0 font-mono text-xs text-faint">
          {position}/{total}
        </span>
      </header>

      {anchor ? (
        <section className="flex items-start justify-between gap-3">
          <Sentence context={anchor} className="text-2xl leading-snug" />
          <Speak url={sentenceUrl} label="Hear the sentence" />
        </section>
      ) : (
        <p className="text-2xl leading-snug">{word.lemma}</p>
      )}

      {asking && (
        <section className="flex flex-col gap-6">
          <p className="text-xl text-dim">
            What does <span className="font-semibold text-text">{anchor?.form ?? word.lemma}</span> mean here?
          </p>
          <Button onClick={reveal} variant="ghost" size={null} className="px-4 py-4 text-xl text-text">
            Show what it means
          </Button>
          {due.pace && <p className="text-xs text-faint">Usually {seconds(due.pace.typical_ms)}</p>}
        </section>
      )}

      {shown && (
        <section className="flex flex-col gap-4">
          {anchor && <p className="text-xl text-dim">{anchor.translation}</p>}
          <div className="flex flex-col gap-1 border-t border-line pt-4">
            <div className="flex items-center justify-between gap-3">
              <p className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <span className="text-2xl font-semibold">{word.lemma}</span>
                {word.ipa && <span className="text-lg text-dim">/{word.ipa}/</span>}
              </p>
              <Speak url={wordUrl} label={`Hear "${word.lemma}" slowly`} />
            </div>
            <p className="text-xl text-accent">{word.gloss}</p>
            {commonness(word) && <p className="text-sm text-faint">{commonness(word)}</p>}
          </div>
          {others.length > 0 && <Elsewhere contexts={others} language={word.languages.target} />}
        </section>
      )}

      {phase.kind === 'meeting' && (
        <Button onClick={() => setPhase({ kind: 'asking' })} variant="primary" size={null} className="px-4 py-4 text-xl">
          Got it - ask me
        </Button>
      )}

      {phase.kind === 'revealed' && (
        <section className="flex flex-col gap-3">
          <p className="text-xl text-dim">Did it come?</p>
          {ratings.map(({ rating, label, hint }) => (
            <Button
              key={rating}
              onClick={() => grade(rating)}
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

      {phase.kind === 'graded' && (
        <section className="flex flex-col gap-4">
          <p className="text-xl">
            {stitchLine(phase.reviewed.stitch)} Back {whenBack(phase.reviewed.interval_days)}.
          </p>
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

/** A sentence with its word marked. */
function Sentence({ context, className = '' }: { context: Context; className?: string }) {
  const { before, word, after } = mark(context.text, context.start, context.form)
  return (
    <p className={`flex-1 ${className}`}>
      {before}
      {word && <mark className="rounded-xs bg-accent-soft px-0.5 text-accent">{word}</mark>}
      {after}
    </p>
  )
}

/**
 * The other sentences the word has been met in: one word, several
 * contexts, and the meaning is what they have in common.
 */
function Elsewhere({ contexts, language }: { contexts: Context[]; language: string }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-sm font-medium tracking-caption text-dim uppercase">Also met in</h3>
      <ul className="flex flex-col gap-2">
        {contexts.map((context) => (
          <li key={context.sentence} className="flex items-start gap-3 rounded-md bg-raise px-3 py-2">
            <div className="flex flex-1 flex-col gap-0.5">
              <Sentence context={context} className="text-lg" />
              <span className="text-base text-dim">{context.translation}</span>
            </div>
            <Speak url={speechUrl(language, context.text, 'normal')} label={`Hear "${context.text}"`} />
          </li>
        ))}
      </ul>
    </section>
  )
}
