import { Suspense, use, useState } from 'react'

import { api, modelUrl, type Standings } from '@/api'
import { Shell } from '@/App'
import { change, dayOf, endOf, lines, type Day } from '@/then'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/**
 * Then and now: where the learner stood on a day of their choosing, beside
 * where they stand today - and the whole of it, as a file to keep.
 *
 * The past is not stored: it is the answers given by then, played through
 * the scheduler again, so any day since the first answer can be asked about.
 */
export function ThenAndNowScreen({ first, onLeave }: { first: Promise<Standings>; onLeave: () => void }) {
  const opened = use(first)
  const [day, setDay] = useState<Day>(dayOf(opened.then.at))
  const [standings, setStandings] = useState(first)
  const today = dayOf(new Date())
  const earliest = opened.began ? dayOf(opened.began) : undefined

  const look = (next: Day) => {
    setDay(next)
    if (next) setStandings(api.standing(endOf(next)))
  }

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Then and now</h2>
        <p className="text-lg text-dim">
          Where you stood at the end of a day, beside where you stand today - worked out from your answers, so any day since
          the first can be looked at.
        </p>
      </section>

      <label className="flex flex-col gap-2">
        <span className="text-base text-dim">On the evening of</span>
        <Input
          type="date"
          value={day}
          min={earliest}
          max={today}
          onChange={(event) => look(event.target.value)}
          // text-lg: a phone zooms into a field whose text is under 16px.
          className="h-auto px-3 py-3 text-lg"
        />
      </label>

      <Suspense fallback={<p className="text-lg text-faint">Working it out…</p>}>
        <Comparison standings={standings} />
      </Suspense>

      <section className="flex flex-col gap-2 rounded-md bg-raise p-4">
        <h3 className="text-base font-medium tracking-caption text-dim uppercase">Your learner model</h3>
        <p className="text-base text-dim">
          Everything hilvan knows about you, in one file: every answer and what it did, your own words, what you said you
          know. Keep it, or move it to another stand with <code className="font-mono text-sm">hilvan import</code>.
        </p>
        <div className="flex flex-wrap gap-x-5 gap-y-2">
          <a href={modelUrl()} download className="text-lg text-text underline">
            Download it as it stands
          </a>
          {day && day < today && (
            <a href={modelUrl(endOf(day))} download className="text-lg text-text underline">
              As it stood on {day}
            </a>
          )}
        </div>
      </section>

      <footer className="mt-auto pt-6">
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back to today
        </Button>
      </footer>
    </Shell>
  )
}

function Comparison({ standings }: { standings: Promise<Standings> }) {
  const { then, now } = use(standings)
  return (
    <div className="grid grid-cols-[1fr_auto_auto_auto] items-baseline gap-x-4 gap-y-2">
      <span />
      <span className="text-right text-xs tracking-caption text-faint uppercase">then</span>
      <span className="text-right text-xs tracking-caption text-faint uppercase">now</span>
      <span />
      {lines(then, now).map((line) => (
        <Row key={line.label} label={line.label} then={line.then} now={line.now} moved={change(line)} />
      ))}
    </div>
  )
}

function Row({ label, then, now, moved }: { label: string; then: number; now: number; moved: string }) {
  return (
    <>
      <span className="text-lg">{label}</span>
      <span className="text-right font-mono text-lg tabular-nums text-dim">{then}</span>
      <span className="text-right font-mono text-lg tabular-nums">{now}</span>
      <span className={`min-w-12 text-right font-mono text-base tabular-nums ${moved.startsWith('-') ? 'text-bad' : 'text-good'}`}>{moved}</span>
    </>
  )
}
