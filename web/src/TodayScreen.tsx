import type { Counts, Stitch, Today } from '@/api'
import { Shell } from '@/App'
import { Button } from '@/components/ui/button'

/** The word the learner sees for each state, and what it means. */
const stitches = [
  { key: 'new', label: 'new', meaning: 'not started' },
  { key: 'basted', label: 'basted', meaning: 'tacked in place' },
  { key: 'sewn', label: 'sewn', meaning: 'holding' },
] as const satisfies readonly { key: Stitch; label: string; meaning: string }[]

/**
 * What is waiting today.
 *
 * The queue is reviews first and at most one new formula, which is the whole
 * of the product's promise about pace: nothing new while something old is
 * still slipping.
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
  const fresh = today.queue.filter((due) => due.is_new).length
  const backwards = today.queue.filter((due) => due.direction === 'recognise').length

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Today</h2>
        <p className="text-xl text-dim">
          {waiting === 0
            ? today.reviewed_today > 0
              ? `Done for today - ${today.reviewed_today} answered. Come back tomorrow.`
              : 'Nothing is waiting. Load a pack to start.'
            : describeQueue(waiting, fresh, backwards)}
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

function describeQueue(waiting: number, fresh: number, backwards: number): string {
  const reviews = waiting - fresh
  const tail = backwards > 0 ? ` ${backwards} of them asked backwards.` : ''
  if (reviews === 0) return `One new formula to start.${tail}`
  const plural = reviews === 1 ? 'formula' : 'formulas'
  return fresh === 0
    ? `${reviews} ${plural} to come back to.${tail}`
    : `${reviews} ${plural} to come back to, then one new one.${tail}`
}
