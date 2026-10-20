import type { Counts, Due, Stitch, Words } from '@/api'
import { Shell, type Day } from '@/App'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { useDelivery } from '@/delivery'
import { useKeys } from '@/keys'
import { awayLine, breakLine, roomLine } from '@/schedule'
import { fitting, minutesOf } from '@/sitting'
import { asOfLine, describeQueue, nextLine, waitingLine } from '@/today'
import { levelLine, levelName } from '@/words'

/** The short sittings offered: a stop or two, a coffee, a ride. */
const SHORT_SITTINGS = [5, 10, 15] as const

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
 * old is still slipping. A day that begins owing cards left over from
 * earlier ones opens fewer new ones, and says so; a break empties it.
 */
export function TodayScreen({
  day,
  onStart,
  onListen,
  onVoices,
  onOwnWords,
  onInventory,
  onThen,
  onTakeBreak,
  onBack,
  onCountAway,
  onDrift,
  onDevice,
}: {
  day: Day
  /** Starts a sitting over these cards: the whole queue, or what fits a few minutes. */
  onStart: (items: Due[]) => void
  onListen: () => void
  onVoices: () => void
  onOwnWords: () => void
  onInventory: () => void
  onThen: () => void
  onTakeBreak: () => void
  /** The learner is back from the break they are on. */
  onBack: (id: number) => void
  /** The time since the last answer is counted as a break. */
  onCountAway: () => void
  onDrift: () => void
  /** What this device keeps, how it looks, and signing it out. */
  onDevice: () => void
}) {
  const { read, queue } = day
  const today = read.today
  const delivery = useDelivery()
  const started = today.progress.produce.basted + today.progress.produce.sewn + today.progress.recognise.basted + today.progress.recognise.sewn > 0
  const waiting = queue.length
  const answeredHere = today.queue.length - queue.length
  const owing = roomLine(today.room)
  const sending = waitingLine(delivery)

  useKeys(waiting > 0 && !today.on_break ? { space: () => onStart(queue) } : {})

  // The thing the screen is for, where the thumb is: start, or come back
  // from a break.
  const actions = today.on_break ? (
    <Button onClick={() => onBack(today.on_break!.id)} variant="primary" size={null} className="px-4 py-4 text-xl">
      I&apos;m back
    </Button>
  ) : waiting > 0 ? (
    <>
      {minutesOf(queue, today.usual) > SHORT_SITTINGS[0] && (
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
          <span className="text-base text-faint">Only a few minutes?</span>
          <div className="flex flex-1 justify-end gap-2">
            {SHORT_SITTINGS.map((minutes) => (
              <Button
                key={minutes}
                onClick={() => onStart(fitting(queue, minutes, today.usual))}
                variant="ghost"
                size={null}
                className="px-3 py-2 text-base text-text"
              >
                {minutes} min
              </Button>
            ))}
          </div>
        </div>
      )}
      <Button onClick={() => onStart(queue)} variant="primary" size={null} className="px-4 py-4 text-xl">
        Start <span className="text-base font-normal opacity-80">· about {minutesOf(queue, today.usual)} min</span>
      </Button>
    </>
  ) : null

  return (
    <Shell actions={actions}>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Today</h2>
        {read.cached && <p className="text-base text-warn">{asOfLine(today.as_of)}</p>}
        <p className="text-xl text-dim">
          {today.on_break
            ? breakLine(today.on_break)
            : waiting === 0
              ? today.reviewed_today + answeredHere > 0
                ? `Done for today - ${today.reviewed_today + answeredHere} answered. Come back tomorrow.`
                : 'Nothing is waiting. Load a pack to start.'
              : describeQueue(queue)}
        </p>
        {owing && !today.on_break && <p className="text-base text-faint">{owing}</p>}
        {sending && <p className="text-base text-faint">{sending}</p>}
      </section>

      {/* An absence nobody declared: one tap makes it a break, and what came
          due in it stops being owed. Offered until the first answer. */}
      {today.away_since && (
        <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
          <p className="text-lg text-dim">{awayLine(today.away_since)}</p>
          <Button onClick={onCountAway} variant="ghost" size={null} className="self-start px-4 py-3 text-lg text-text">
            Count it as a break
          </Button>
        </section>
      )}

      {/* Listening is practice on what has been met, so it waits for the
          first formula; before that there is nothing to hear. */}
      {started && (
        <Button onClick={onListen} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
          <span className="block text-xl font-medium">Listen</span>
          <span className="block text-base text-faint">Hear a sentence, recall what it means, then look. Nothing is graded.</span>
        </Button>
      )}

      {/* Starting from what is known: the commonest words, one question
          each. Offered here for the start; once a word is declared known,
          "Word check" in the footer is the way back to it. */}
      {inventoryOpen(today.words) && (
        <Button onClick={onInventory} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
          <span className="block text-xl font-medium">What you already know</span>
          <span className="block text-base text-faint">The commonest words, one at a time. The ones you know start sewn.</span>
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
        {!today.on_break && nextLine(today.next, today.room) && <p className="text-base text-faint">{nextLine(today.next, today.room)}</p>}
      </section>

      <YourWords words={today.words} />

      <footer className="mt-auto flex flex-wrap gap-x-5 gap-y-2">
        <Button onClick={onOwnWords} variant="link" className="text-base text-faint underline">
          Your own words
        </Button>
        <Button onClick={onInventory} variant="link" className="text-base text-faint underline">
          Word check
        </Button>
        <Button onClick={onThen} variant="link" className="text-base text-faint underline">
          Then and now
        </Button>
        <Button onClick={onDrift} variant="link" className="text-base text-faint underline">
          What slipped
        </Button>
        {!today.on_break && (
          <Button onClick={onTakeBreak} variant="link" className="text-base text-faint underline">
            Take a break
          </Button>
        )}
        <Button onClick={onVoices} variant="link" className="text-base text-faint underline">
          Voices
        </Button>
        <Button onClick={onDevice} variant="link" className="text-base text-faint underline">
          This device
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

/**
 * Whether the inventory is offered on Today: to a learner who has not yet
 * said they know a word, while the commonest thousand are not all in hand.
 */
function inventoryOpen(words: Words): boolean {
  const first = words.levels[0]
  return first !== undefined && first.sewn + first.basted < first.size && words.declared === 0
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
      {words.declared > 0 && (
        <p className="text-base text-faint">
          {words.declared} of them on your word alone - each is asked when a sentence brings it.
        </p>
      )}
      {words.waiting > 0 && (
        <p className="text-base text-faint">
          {words.waiting} met in a sentence and waiting their turn - a few open each day.
        </p>
      )}
    </section>
  )
}
