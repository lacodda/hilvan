import { use } from 'react'

import type { Drift, Week } from '@/api'
import { Shell } from '@/App'
import { dayName, slipDetail, slipNow } from '@/schedule'
import { Button } from '@/components/ui/button'

/**
 * What slipped: what the learner had come up with on an earlier day, and
 * then could not - this week and last.
 *
 * Nothing to do here: what slipped comes back sooner on its own. The list is
 * for knowing it, which is worth something on its own - a formula that
 * slips every week is a formula to look at again.
 */
export function DriftScreen({ drift, onLeave }: { drift: Promise<Drift>; onLeave: () => void }) {
  const { this_week, last_week } = use(drift)

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">What slipped</h2>
        <p className="text-lg text-dim">
          What you had come up with on an earlier day and then could not. It comes back sooner now, on its own.
        </p>
      </section>

      <Slips title="This week" week={this_week} />
      <Slips title="Last week" week={last_week} />

      <footer className="mt-auto pt-6">
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back to today
        </Button>
      </footer>
    </Shell>
  )
}

function Slips({ title, week }: { title: string; week: Week }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-base font-medium tracking-caption text-dim uppercase">
        {title} <span className="normal-case tracking-normal text-faint">- from {dayName(`${week.starts_on}T12:00:00`)}</span>
      </h3>
      {week.slips.length === 0 ? (
        <p className="text-lg text-faint">Nothing slipped.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {week.slips.map((slip) => (
            <li key={`${slip.kind}:${slip.id}:${slip.direction ?? ''}`} className="flex items-baseline justify-between gap-3 rounded-md bg-raise px-3 py-2">
              <span className="flex flex-col">
                <span className="text-xl">{slip.name}</span>
                <span className="text-base text-faint">{slipDetail(slip)}</span>
              </span>
              <span className="shrink-0 text-base text-dim">{slipNow(slip)}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
