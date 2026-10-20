import { useEffect, useState } from 'react'

import { api } from '@/api'
import { forgetPlan, Shell, type Day } from '@/App'
import { Button } from '@/components/ui/button'
import { Segment, SegmentedControl } from '@/components/ui/segmented-control'
import { drain, useDelivery } from '@/delivery'
import { canHold, forgetHeld, heldOf, setBadge } from '@/offline'
import { clear, pending } from '@/outbox'
import { soundsOf } from '@/sitting'
import { THEMES, useTheme, type Theme } from '@/theme'
import { waitingLine } from '@/today'

const THEME_LABELS: Record<Theme, string> = { system: 'As the device', dark: 'Dark', light: 'Light' }

/**
 * This device: how the tutor looks on it, what it keeps for a sitting out of
 * reach, the answers still on their way, and signing it out.
 */
export function DeviceScreen({ day, onLeave, onSignedOut }: { day: Day; onLeave: () => void; onSignedOut: () => void }) {
  const { theme, setTheme } = useTheme()
  const delivery = useDelivery()
  const sending = waitingLine(delivery)
  const [sounds] = useState(() => soundsOf(day.queue, day.plan))
  const [kept, setKept] = useState<number | null>(null)
  const [signingOut, setSigningOut] = useState<string | null>(null)

  useEffect(() => {
    void heldOf(sounds).then(setKept)
  }, [sounds])

  const signOut = () => {
    setSigningOut('Sending what is waiting…')
    void (async () => {
      await drain()
      const left = (await pending()).length
      if (left > 0) {
        // Signing out forgets what the device keeps; answers not yet with
        // the tutor would go with it.
        setSigningOut(
          `${left === 1 ? '1 answer has' : `${left} answers have`} not reached the tutor yet. Sign out once ${left === 1 ? 'it has' : 'they have'}, or they are lost.`,
        )
        return
      }
      await api.logOut()
      await Promise.all([clear(), forgetHeld(), setBadge(0, [], Date.now())])
      forgetPlan()
      onSignedOut()
    })().catch((error: unknown) => setSigningOut(error instanceof Error ? error.message : String(error)))
  }

  return (
    <Shell
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back
        </Button>
      }
    >
      <h2 className="text-2xl font-semibold tracking-tight">This device</h2>

      <section className="flex flex-col gap-3">
        <h3 className="text-base font-medium tracking-caption text-dim uppercase">Look</h3>
        <SegmentedControl aria-label="Theme" value={theme} onValueChange={(value) => setTheme(value as Theme)}>
          {THEMES.map((option) => (
            <Segment key={option} value={option}>
              {THEME_LABELS[option]}
            </Segment>
          ))}
        </SegmentedControl>
        <p className="text-base text-faint">Kept on this device only: a phone at night and a laptop by a window can each have their own.</p>
      </section>

      <section className="flex flex-col gap-3">
        <h3 className="text-base font-medium tracking-caption text-dim uppercase">Out of reach</h3>
        {canHold() ? (
          <p className="text-lg text-dim">
            The app and today&apos;s cards are kept for a sitting where the tutor cannot be reached
            {kept !== null && sounds.length > 0 && (
              <>
                , with {kept} of the {sounds.length} sounds it plays
              </>
            )}
            . Answers given there wait here and go in the order they were given.
          </p>
        ) : (
          <p className="text-lg text-dim">
            A sitting out of reach needs the tutor&apos;s https:// address: over plain http:// the browser keeps nothing for the
            road, and the app works only while the tutor can be reached.
          </p>
        )}
        {sending ? <p className="text-base text-faint">{sending}</p> : <p className="text-base text-faint">Every answer has reached the tutor.</p>}
        {delivery.waiting > 0 && (
          <Button onClick={() => void drain()} variant="ghost" size={null} className="self-start px-4 py-2 text-base text-text">
            Send them now
          </Button>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h3 className="text-base font-medium tracking-caption text-dim uppercase">Sign out</h3>
        <p className="text-base text-faint">Signing out forgets what this device keeps of your study.</p>
        <Button onClick={signOut} variant="ghost" size={null} className="self-start px-4 py-2 text-base text-text">
          Sign out of this device
        </Button>
        {signingOut && <p className="text-base text-dim">{signingOut}</p>}
      </section>
    </Shell>
  )
}
