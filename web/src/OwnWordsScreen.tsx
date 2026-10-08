import { use, useState, type FormEvent } from 'react'

import { api, type OwnKind, type OwnWord } from '@/api'
import { Shell } from '@/App'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Segment, SegmentedControl } from '@/components/ui/segmented-control'

/** How each kind of word is typed, said where it is typed. */
const kinds: Record<OwnKind, { label: string; hint: string; example: string }> = {
  verb: { label: 'Verb', hint: 'The plain form: swim, cook, drive.', example: 'swim' },
  noun: { label: 'Noun', hint: 'As you would say it after "I have": a car, two dogs, some time.', example: 'a car' },
}

/**
 * Words from the learner's own life, put into the drill.
 *
 * The pack says "I can swim" because its author chose "swim"; a verb the
 * learner actually does is remembered faster. A verb goes into the formulas
 * built on a plain verb - can, must, will, want to - and a noun into those
 * built on a thing - have got, would like. They come up in about half of the
 * substitutions of a formula that takes them.
 */
export function OwnWordsScreen({ words, onLeave }: { words: Promise<OwnWord[]>; onLeave: () => void }) {
  // Read once; after that the list is kept here from what the server answers,
  // so adding a word does not blank the screen to read it all again.
  const [listed, setListed] = useState(use(words))
  const [kind, setKind] = useState<OwnKind>('verb')
  const [native, setNative] = useState('')
  const [target, setTarget] = useState('')
  const [saving, setSaving] = useState(false)
  const [refused, setRefused] = useState<string | null>(null)
  const [removing, setRemoving] = useState<number | null>(null)

  const add = (event: FormEvent) => {
    event.preventDefault()
    setSaving(true)
    setRefused(null)
    void api
      .addOwnWord(kind, native, target)
      .then((added) => {
        setNative('')
        setTarget('')
        setListed((before) => [added, ...before])
      })
      // The server says what is wrong in a sentence meant for the learner.
      .catch((error: unknown) => setRefused(error instanceof Error ? error.message : 'The word could not be added.'))
      .finally(() => setSaving(false))
  }

  const remove = (word: OwnWord) => {
    setRemoving(word.id)
    void api
      .removeOwnWord(word.id)
      .then(() => setListed((before) => before.filter((other) => other.id !== word.id)))
      .catch(() => setRefused(`"${word.target}" could not be removed.`))
      .finally(() => setRemoving(null))
  }

  return (
    <Shell>
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Your own words</h2>
        <p className="text-lg text-dim">
          Nouns and verbs from your own life. The drill puts them into the formulas that can take them, about half the time.
        </p>
      </section>

      <form onSubmit={add} className="flex flex-col gap-3 rounded-md bg-raise p-4">
        <SegmentedControl aria-label="Kind of word" value={kind} onValueChange={(value) => setKind(value as OwnKind)}>
          {(Object.keys(kinds) as OwnKind[]).map((key) => (
            <Segment key={key} value={key}>
              {kinds[key].label}
            </Segment>
          ))}
        </SegmentedControl>
        <p className="text-base text-faint">{kinds[kind].hint}</p>
        <Input
          value={target}
          onChange={(event) => setTarget(event.target.value)}
          aria-label="In English"
          placeholder={`In English - ${kinds[kind].example}`}
          lang="en"
          autoComplete="off"
          // text-lg: a phone zooms into a field whose text is under 16px.
          className="h-auto px-3 py-3 text-lg"
        />
        <Input
          value={native}
          onChange={(event) => setNative(event.target.value)}
          aria-label="In your language"
          placeholder="In your language"
          autoComplete="off"
          className="h-auto px-3 py-3 text-lg"
        />
        <Button
          type="submit"
          disabled={saving || native.trim() === '' || target.trim() === ''}
          variant="primary"
          size={null}
          className="px-4 py-3 text-lg"
        >
          {saving ? 'Adding…' : 'Add'}
        </Button>
        {refused && <p className="text-base text-bad">{refused}</p>}
      </form>

      {listed.length === 0 ? (
        <p className="text-lg text-dim">No words of your own yet.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {listed.map((word) => (
            <li key={word.id} className="flex items-center gap-3 rounded-md bg-raise px-3 py-2">
              <span className="w-12 shrink-0 text-sm text-faint">{word.kind}</span>
              <span className="flex flex-1 flex-col">
                <span className="text-lg">{word.target}</span>
                <span className="text-base text-dim">{word.native}</span>
                {!word.known && <span className="text-sm text-warn">Not a word hilvan knows - check the spelling.</span>}
              </span>
              <Button
                onClick={() => remove(word)}
                disabled={removing !== null}
                variant="link"
                className="text-base text-faint underline"
                aria-label={`Remove ${word.target}`}
              >
                {removing === word.id ? 'Removing…' : 'Remove'}
              </Button>
            </li>
          ))}
        </ul>
      )}

      <footer className="mt-auto pt-6">
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Back to today
        </Button>
      </footer>
    </Shell>
  )
}
