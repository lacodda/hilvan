import { useState, type FormEvent } from 'react'

import { api } from '@/api'
import { Shell } from '@/App'
import { startOf } from '@/schedule'
import { dayOf, type Day } from '@/then'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/**
 * Taking a break: the schedule waits while the learner is away.
 *
 * The day back is optional. Without one the break lasts until "I'm back" on
 * Today - which is also what any answer given meanwhile says.
 */
export function BreakScreen({ onTaken, onLeave }: { onTaken: () => void; onLeave: () => void }) {
  const [back, setBack] = useState<Day>('')
  const [saving, setSaving] = useState(false)
  const [failed, setFailed] = useState<string | null>(null)
  const now = new Date()
  const tomorrow = dayOf(new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1))

  const take = (event: FormEvent) => {
    event.preventDefault()
    setSaving(true)
    setFailed(null)
    api
      .takeBreak(back ? startOf(back) : null)
      .then(onTaken)
      .catch((error: unknown) => {
        setFailed(error instanceof Error ? error.message : String(error))
        setSaving(false)
      })
  }

  return (
    <Shell>
      <section className="flex flex-col gap-3">
        <h2 className="text-2xl font-semibold tracking-tight">Take a break</h2>
        <p className="text-lg text-dim">
          The schedule waits while you are away. Nothing comes due, and when you are back it picks up where it was - the
          same cards, in the same order, at the same pace - instead of everything that would have come back at once.
        </p>
        <p className="text-base text-faint">
          Memory does not stop for a holiday, so some of it will come back loose. That is what the first days back are for.
        </p>
      </section>

      <form onSubmit={take} className="flex flex-col gap-4">
        <label className="flex flex-col gap-2">
          <span className="text-base text-dim">Back on - leave it empty if you do not know yet</span>
          <Input
            type="date"
            value={back}
            min={tomorrow}
            onChange={(event) => setBack(event.target.value)}
            // text-lg: a phone zooms into a field whose text is under 16px.
            className="h-auto px-3 py-3 text-lg"
          />
        </label>
        <Button type="submit" disabled={saving} variant="primary" size={null} className="px-4 py-4 text-xl">
          {saving ? 'Starting…' : 'Start the break'}
        </Button>
        {failed && <p className="text-base text-bad">{failed}</p>}
      </form>

      <footer className="mt-auto pt-6">
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Not now
        </Button>
      </footer>
    </Shell>
  )
}
