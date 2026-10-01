import type { Counts, Stitch, Today, Words } from '@/api'
import { Shell } from '@/App'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { describeQueue } from '@/today'
import { levelLine, levelName } from '@/words'

/** The word the learner sees for each state, and what it means. */
const stitches = [
  { key: 'new', label: 'new', meaning: 'not started' },
  { key: 'basted', label: 'basted', meaning: 'tacked in place' },
  { key: 'sewn', label: 'sewn', meaning: 'holding' },
] as const satisfies readonly { key: Stitch; label: string; meaning: string }[]

/**
 * What is waiting today.
 *
 * The queue is reviews first and at most one new formula, then the words -
 * theirs first again, then the new ones the day has room for - which is the
 * whole of the product's promise about pace: nothing new while something
 * old is still slipping.
 */
export function TodayScreen({
  today,
  onStart,
  onListen,
  onVoices,
  onSignOut,
}: {
  today: Today
  onStart: () => void
  onListen: () => void
  onVoices: () => void
  onSignOut: () => void
}) {
  const started = today.progress.produce.basted + today.progress.produce.sewn + today.progress.recognise.basted + today.progress.recognise.sewn > 0
  const waiting = today.queue.length

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Today</h2>
        <p className="text-xl text-dim">
          {waiting === 0
            ? today.reviewed_today > 0
              ? `Done for today - ${today.reviewed_today} answered. Come back tomorrow.`
              : 'Nothing is waiting. Load a pack to start.'
            : describeQueue(today.queue)}
        </p>
      </section>

      {waiting > 0 && (
        <Button
          onClick={onStart}
          variant="primary" size={null} className="px-4 py-4 text-xl"
        >
          Start
        </Button>
      )}

      {/* Listening is practice on what has been met, so it waits for the
          first formula; before that there is nothing to hear. */}
      {started && (
        <Button onClick={onListen} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
          <span className="block text-xl font-medium">Listen</span>
          <span className="block text-base text-faint">Hear a sentence, recall what it means, then look. Nothing is graded.</span>
        </Button>
      )}

      <section className="flex flex-col gap-3">
        <h3 className="text-base font-medium tracking-caption text-dim uppercase">Your formulas</h3>
        <div className="grid grid-cols-[1fr_auto_auto] items-baseline gap-x-4 gap-y-2">
          {/* Saying it and understanding it, side by side. Recognition always
              runs ahead, and the gap between the columns is the honest
              picture one averaged number would hide. */}
          <span />
          <span className="text-xs tracking-caption text-faint uppercase">say</span>
          <span className="text-xs tracking-caption text-faint uppercase">know</span>
          {stitches.map(({ key, label, meaning }) => (
            <Row
              key={key}
              label={label}
              meaning={meaning}
              produce={today.progress.produce[key]}
              recognise={today.progress.recognise[key]}
            />
          ))}
        </div>
        {behind(today.progress.produce, today.progress.recognise) && (
          <p className="text-base text-faint">
            You understand more than you can say, which is how it goes. The drill asks both ways.
          </p>
        )}
        {today.reviewed_today > 0 && waiting > 0 && (
          <p className="text-base text-faint">{today.reviewed_today} answered so far today.</p>
        )}
      </section>

      <YourWords words={today.words} />

      <footer className="mt-auto flex gap-5 pt-6">
        <Button onClick={onVoices} variant="link" className="text-base text-faint underline">
          Voices
        </Button>
        <Button onClick={onSignOut} variant="link" className="text-base text-faint underline">
          Sign out
        </Button>
      </footer>
    </Shell>
  )
}

function Row({
  label,
  meaning,
  produce,
  recognise,
}: {
  label: string
  meaning: string
  produce: number
  recognise: number
}) {
  return (
    <>
      <span className="text-xl">
        {label} <span className="text-base text-faint">- {meaning}</span>
      </span>
      <span className="text-right font-mono text-xl tabular-nums">{produce}</span>
      <span className="text-right font-mono text-xl tabular-nums text-dim">{recognise}</span>
    </>
  )
}

/** Whether understanding has genuinely pulled ahead of saying. */
function behind(produce: Counts, recognise: Counts): boolean {
  return recognise.sewn > produce.sewn
}

/**
 * The words in hand, against the levels a learner aims at.
 *
 * The bar is what is sewn - held over long intervals - out of the whole
 * level, the commonest thousand words of the language rather than the words
 * of the pack: a level is the language's, and the number that grows toward
 * it is the honest one even while it is small.
 */
function YourWords({ words }: { words: Words }) {
  const total = words.counts.new + words.counts.basted + words.counts.sewn
  if (total === 0) return null
  const started = words.counts.basted + words.counts.sewn

  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-base font-medium tracking-caption text-dim uppercase">Your words</h3>
      <p className="text-xl">
        {started === 0 ? 'None started yet' : `${words.counts.sewn} sewn, ${words.counts.basted} basted`}
        <span className="text-base text-faint"> - {total} in your formulas' sentences</span>
      </p>
      <div className="flex flex-col gap-3">
        {words.levels.map((level) => (
          <Progress key={level.size} value={level.sewn} max={level.size} size="sm" label={`The ${levelName(level.size)} level`}>
            <span>
              <span className="font-mono font-semibold text-text">{levelName(level.size)}</span> {levelLine(level)}
            </span>
          </Progress>
        ))}
      </div>
      {words.waiting > 0 && (
        <p className="text-base text-faint">
          {words.waiting} met in a sentence and waiting their turn - a few open each day.
        </p>
      )}
    </section>
  )
}
