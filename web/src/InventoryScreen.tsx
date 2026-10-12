import { use, useEffect, useRef, useState } from 'react'

import { api, type InventoryItem, type InventoryRun } from '@/api'
import { Shell } from '@/App'
import { currentLevel, keyAnswer, nextRequest, ordinal, reachLine, stillToAsk, withdraw, type Op } from '@/inventory'
import { levelName } from '@/words'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'

/** One answer given on this screen, kept so it can be taken back. */
interface Given {
  item: InventoryItem
  known: boolean
}

/**
 * What you already know: the commonest words of the language, one at a
 * time, and one question about each.
 *
 * A word you know starts sewn and is checked when a sentence of the drill
 * brings it; a word you do not is never asked here again. The screen keeps
 * going while the answers travel - a thousand words is an evening only if
 * the thumb never waits for the network - and stops, saying so, only when
 * the server cannot be reached.
 */
export function InventoryScreen({ run, onLeave }: { run: Promise<InventoryRun>; onLeave: () => void }) {
  const [server, setServer] = useState(use(run))
  const [given, setGiven] = useState<Given[]>([])
  const [returned, setReturned] = useState<InventoryItem[]>([])
  const [stuck, setStuck] = useState(false)
  // How many answers and takings-back the server has not confirmed: what the
  // screen says it is still saving once the words run out.
  const [unsent, setUnsent] = useState(0)
  // What has not been heard back about, in order, and how many of those ops
  // the request on its way carries. Read and written only in handlers.
  const pending = useRef<Op[]>([])
  const sending = useRef(0)

  const answered = new Set(given.map((one) => one.item.word))
  const queue = stillToAsk(server.next, returned, answered)
  const current = queue[0]
  const level = currentLevel(server.levels)

  const pump = () => {
    if (sending.current > 0) return
    const request = nextRequest(pending.current)
    if (!request) return
    sending.current = request.ops
    const call = 'undo' in request ? api.undoInventory(request.undo) : api.answerInventory(request.answers)
    call
      .then((after) => {
        pending.current = pending.current.slice(request.ops)
        sending.current = 0
        setUnsent(pending.current.length)
        setStuck(false)
        setServer(after)
        pump()
      })
      .catch(() => {
        sending.current = 0
        setStuck(true)
      })
  }

  const answer = (known: boolean) => {
    if (!current || stuck) return
    setGiven((before) => [...before, { item: current, known }])
    setReturned((before) => before.filter((item) => item.word !== current.word))
    pending.current = [...pending.current, { kind: 'answer', answer: { word: current.word, known } }]
    setUnsent(pending.current.length)
    pump()
  }

  const undo = () => {
    const last = given.at(-1)
    if (!last || stuck) return
    setGiven((before) => before.slice(0, -1))
    setReturned((before) => [...before, last.item])
    pending.current = withdraw(pending.current, last.item.word, sending.current) ?? [
      ...pending.current,
      { kind: 'undo', word: last.item.word },
    ]
    setUnsent(pending.current.length)
    pump()
  }

  // Keys on a desktop: right or Y for "I know it", left or N for "no",
  // backspace to take the last answer back.
  const keys = useRef({ answer, undo })
  useEffect(() => {
    keys.current = { answer, undo }
  })
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return
      const what = keyAnswer(event.code)
      if (!what) return
      event.preventDefault()
      if (what === 'undo') keys.current.undo()
      else keys.current.answer(what === 'know')
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const last = given.at(-1)

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">What you already know</h2>
        <p className="text-lg text-dim">
          The commonest words, one at a time. Say you know one if you would understand it in a sentence: it starts sewn,
          and the drill checks it when a sentence brings it.
        </p>
      </section>

      {level && (
        <Progress
          value={level.size - level.left}
          max={level.size}
          size="sm"
          label={`The ${levelName(level.size)} level`}
        >
          <span>{reachLine(level)}</span>
        </Progress>
      )}

      {current ? (
        <section className="flex flex-col gap-6 rounded-md bg-raise px-4 py-8">
          <div className="flex flex-col items-center gap-2 text-center">
            <p lang={server.language ?? undefined} className="text-4xl font-semibold tracking-tight break-words">
              {current.lemma}
            </p>
            {current.ipa && <p className="font-mono text-lg text-dim">/{current.ipa}/</p>}
            <p className="text-sm text-faint">the {ordinal(current.rank)} commonest word</p>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Button onClick={() => answer(false)} disabled={stuck} variant="ghost" size={null} className="px-4 py-4 text-xl text-text">
              No
            </Button>
            <Button onClick={() => answer(true)} disabled={stuck} variant="primary" size={null} className="px-4 py-4 text-xl">
              I know it
            </Button>
          </div>
        </section>
      ) : unsent > 0 ? (
        <p className="text-xl text-dim">Saving your answers…</p>
      ) : (
        <p className="text-xl text-dim">
          {server.language === null
            ? 'No pack is loaded yet, so there is no language to go through.'
            : server.levels.length === 0
              ? 'hilvan has no word list for this language yet.'
              : `That is all ${server.levels.at(-1)?.size ?? ''} - every word of every level is answered.`}
        </p>
      )}

      {stuck && (
        <div className="flex flex-col gap-2 rounded-md bg-raise p-4">
          <p className="text-lg text-bad">Your last answers did not reach the tutor. Nothing is lost - they are kept here.</p>
          <Button onClick={pump} variant="primary" size={null} className="self-start px-4 py-2 text-lg">
            Try again
          </Button>
        </div>
      )}

      <footer className="mt-auto flex flex-wrap items-baseline justify-between gap-x-5 gap-y-2 pt-6">
        {last ? (
          <Button onClick={undo} disabled={stuck} variant="link" className="text-base text-faint underline">
            Take back "{last.item.lemma}"
          </Button>
        ) : (
          <span className="text-base text-faint">
            {server.answered > 0 ? `${server.answered} answered so far, ${server.known} you know` : 'Keys: right for yes, left for no'}
          </span>
        )}
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back to today
        </Button>
      </footer>
    </Shell>
  )
}
