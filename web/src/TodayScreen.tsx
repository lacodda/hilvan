import type { Counts, Stitch, Today } from '@/api'
import { Shell } from '@/App'

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
  onSignOut,
}: {
  today: Today
  onStart: () => void
  onSignOut: () => void
}) {
  const waiting = today.queue.length
  const fresh = today.queue.filter((due) => due.is_new).length
  const backwards = today.queue.filter((due) => due.direction === 'recognise').length

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Today</h2>
        <p className="text-base text-dim">
          {waiting === 0
            ? today.reviewed_today > 0
              ? `Done for today - ${today.reviewed_today} answered. Come back tomorrow.`
              : 'Nothing is waiting. Load a pack to start.'
            : describeQueue(waiting, fresh, backwards)}
        </p>
      </section>

      {waiting > 0 && (
        <button
          type="button"
          onClick={onStart}
          className="rounded-md bg-accent px-4 py-4 text-base font-medium text-on-accent"
        >
          Start
        </button>
      )}

      <section className="flex flex-col gap-3">
        <h3 className="text-sm font-medium tracking-caption text-dim uppercase">Your formulas</h3>
        <div className="grid grid-cols-[1fr_auto_auto] items-baseline gap-x-4 gap-y-2">
          {/* Saying it and understanding it, side by side. Recognition always
              runs ahead, and the gap between the columns is the honest
              picture one averaged number would hide. */}
          <span />
          <span className="text-2xs tracking-caption text-faint uppercase">say</span>
          <span className="text-2xs tracking-caption text-faint uppercase">know</span>
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
          <p className="text-xs text-faint">
            You understand more than you can say, which is how it goes. The drill asks both ways.
          </p>
        )}
        {today.reviewed_today > 0 && waiting > 0 && (
          <p className="text-xs text-faint">{today.reviewed_today} answered so far today.</p>
        )}
      </section>

      <footer className="mt-auto pt-6">
        <button type="button" onClick={onSignOut} className="text-xs text-faint underline">
          Sign out
        </button>
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
      <span className="text-base">
        {label} <span className="text-xs text-faint">- {meaning}</span>
      </span>
      <span className="text-right font-mono text-base tabular-nums">{produce}</span>
      <span className="text-right font-mono text-base tabular-nums text-dim">{recognise}</span>
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
