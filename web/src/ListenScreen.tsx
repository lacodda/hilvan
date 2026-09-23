import { use, useEffect, useState } from 'react'

import { speechUrl, type Heard } from '@/api'
import { Shell } from '@/App'
import { Speak } from '@/Speak'
import { player, useStopOnLeave } from '@/speech'

/**
 * Listening: hear a sentence, recall what it means, then look.
 *
 * Practice rather than a review - nothing here is graded, and the schedule
 * does not move. The sentences are the samples of every formula already
 * started, in an order drawn once when the screen opens.
 */
export function ListenScreen({ sentences, onLeave }: { sentences: Promise<Heard[]>; onLeave: () => void }) {
  const all = use(sentences)
  // Shuffled once: a list in pack order would be recalled by position.
  const [order] = useState(() => shuffle(all))
  const [index, setIndex] = useState(0)
  const [shown, setShown] = useState(false)
  useStopOnLeave()

  const heard = order[index]
  const url = heard ? speechUrl(heard.language, heard.target, 'normal') : null

  // Each sentence is heard before anything is read.
  useEffect(() => {
    if (url) void player.play(url)
  }, [url])

  if (!heard) {
    return (
      <Shell>
        <h2 className="text-2xl font-semibold tracking-tight">Listen</h2>
        <p className="text-[1.0625rem] text-dim">
          {order.length === 0
            ? 'Nothing to listen to yet. Start a formula first - listening draws on what you have met.'
            : `That was all ${order.length}. Come back after the next formula.`}
        </p>
        <button type="button" onClick={onLeave} className="self-start rounded-md bg-accent px-4 py-3 text-[0.9375rem] font-medium text-on-accent">
          Back to today
        </button>
      </Shell>
    )
  }

  return (
    <Shell>
      <header className="flex items-baseline justify-between gap-3">
        <h2 className="text-xl font-semibold tracking-tight">Listen</h2>
        <span className="font-mono text-[0.6875rem] text-faint">
          {index + 1}/{order.length}
        </span>
      </header>
      <p className="text-[0.6875rem] tracking-caption text-faint uppercase">Hear it, recall what it means, then look</p>

      <section className="flex flex-col items-center gap-6 py-6">
        <Speak url={url} label="Hear it again" className="size-16" />
      </section>

      {shown ? (
        <section className="flex flex-col gap-3">
          <p className="text-2xl leading-snug text-accent">{heard.target}</p>
          <p className="border-t border-line pt-3 text-[1.0625rem] text-dim">{heard.native}</p>
          <button
            type="button"
            onClick={() => {
              setShown(false)
              setIndex(index + 1)
            }}
            className="rounded-md bg-accent px-4 py-4 text-[1.0625rem] font-medium text-on-accent"
          >
            Next
          </button>
        </section>
      ) : (
        <button type="button" onClick={() => setShown(true)} className="rounded-md border border-line px-4 py-4 text-[1.0625rem] font-medium">
          Show what it was
        </button>
      )}

      <footer className="mt-auto pt-6">
        <button type="button" onClick={onLeave} className="text-[0.8125rem] text-faint underline">
          Stop for now
        </button>
      </footer>
    </Shell>
  )
}

function shuffle<T>(items: readonly T[]): T[] {
  const copy = [...items]
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[copy[i], copy[j]] = [copy[j]!, copy[i]!]
  }
  return copy
}
