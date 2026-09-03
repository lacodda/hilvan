import type { Today } from '@/api'
import { Shell } from '@/App'

/** The word the learner sees for each state, and what it means. */
const stitches = [
  { key: 'new', label: 'new', meaning: 'not started' },
  { key: 'basted', label: 'basted', meaning: 'tacked in place' },
  { key: 'sewn', label: 'sewn', meaning: 'holding' },
] as const

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

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Today</h2>
        <p className="text-base text-dim">
          {waiting === 0
            ? today.reviewed_today > 0
              ? `Done for today - ${today.reviewed_today} answered. Come back tomorrow.`
              : 'Nothing is waiting. Load a pack to start.'
            : describeQueue(waiting, fresh)}
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
        <ul className="flex flex-col gap-2">
          {stitches.map(({ key, label, meaning }) => (
            <li key={key} className="flex items-baseline justify-between rounded-md bg-raise px-3 py-2">
              <span className="text-base">
                {label} <span className="text-xs text-faint">- {meaning}</span>
              </span>
              <span className="font-mono text-base tabular-nums">{today.counts[key]}</span>
            </li>
          ))}
        </ul>
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

function describeQueue(waiting: number, fresh: number): string {
  const reviews = waiting - fresh
  if (reviews === 0) return 'One new formula to start.'
  const plural = reviews === 1 ? 'formula' : 'formulas'
  return fresh === 0
    ? `${reviews} ${plural} to come back to.`
    : `${reviews} ${plural} to come back to, then one new one.`
}
