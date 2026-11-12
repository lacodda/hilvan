import { Component, Suspense, use, type ReactNode } from 'react'

import { api, speechUrl, type Example, type Hold, type Line, type Token, type Word } from '@/api'
import { Button } from '@/components/ui/button'
import { Drawer, DrawerBody, DrawerHeader, DrawerPopup, DrawerTitle } from '@/components/ui/drawer'
import { Family } from '@/Family'
import { Speak } from '@/Speak'
import { player } from '@/speech'
import { commonness, mark } from '@/words'

/** A running word the learner tapped, in the sentence it stands in. */
export interface Tapped {
  line: Line
  token: Token
}

/** What the sheet can ask the reader to do with a word. */
export interface WordActions {
  /** "I know it". */
  declare: (word: string) => void
  /** Takes "I know it" back. */
  takeBack: (word: string) => void
  /** Into the deck, with the sentence it was tapped in. */
  pick: (word: string, tapped: Tapped) => void
  /** Out of the deck from this sentence. */
  unpick: (word: string, tapped: Tapped) => void
}

// What a word is and where else the learner has it do not change while a
// text is read: asked once a session, and asked again only after a failure.
const words = new Map<string, Promise<Word>>()
const examples = new Map<string, Promise<Example[]>>()

function remembered<T>(cache: Map<string, Promise<T>>, id: string, load: (id: string) => Promise<T>): Promise<T> {
  let promise = cache.get(id)
  if (!promise) {
    promise = load(id).catch((error: unknown) => {
      cache.delete(id)
      throw error
    })
    cache.set(id, promise)
  }
  return promise
}

/** Forgets what was asked about a word: its contexts change when it is taken into the deck. */
export function forget(id: string): void {
  words.delete(id)
}

/**
 * A tapped word, in a sheet from the bottom of the screen.
 *
 * The sentence first - a tap on it says it aloud - then each word the
 * running word is made of ("don't" is two), and for a spelling that is two
 * lemmas ("left") each of them: what it means, how it sounds, how common it
 * is, and what the learner can do with it here. Then the other sentences of
 * their own material it stands in.
 */
export function WordSheet({
  tapped,
  lemmas,
  holds,
  language,
  declaredHere,
  pickedHere,
  actions,
  onClose,
}: {
  tapped: Tapped | null
  lemmas: readonly string[]
  holds: readonly (Hold | null)[]
  language: string
  /** Words declared known on this page, which can be taken back. */
  declaredHere: ReadonlySet<string>
  /** Words taken into the deck on this page, by word and sentence. */
  pickedHere: ReadonlySet<string>
  actions: WordActions
  onClose: () => void
}) {
  return (
    <Drawer open={tapped !== null} onOpenChange={(open) => !open && onClose()}>
      <DrawerPopup side="bottom" size="lg" className="mx-auto max-w-xl">
        {tapped && (
          <Sheet
            tapped={tapped}
            lemmas={lemmas}
            holds={holds}
            language={language}
            declaredHere={declaredHere}
            pickedHere={pickedHere}
            actions={actions}
          />
        )}
      </DrawerPopup>
    </Drawer>
  )
}

/** The key a word taken from a sentence is remembered under on the page. */
export function pickKey(word: string, sentence: number): string {
  return `${word}@${sentence}`
}

function Sheet({
  tapped,
  lemmas,
  holds,
  language,
  declaredHere,
  pickedHere,
  actions,
}: {
  tapped: Tapped
  lemmas: readonly string[]
  holds: readonly (Hold | null)[]
  language: string
  declaredHere: ReadonlySet<string>
  pickedHere: ReadonlySet<string>
  actions: WordActions
}) {
  const { line, token } = tapped
  const chars = Array.from(line.text)
  const spelled = chars.slice(token.start, token.start + token.length).join('')
  const marked = mark(line.text, token.start, spelled)
  const sentenceUrl = speechUrl(language, line.text, 'normal')

  return (
    <>
      <DrawerHeader action={<Speak url={sentenceUrl} label="Hear the sentence" />}>
        <DrawerTitle className="sr-only">{spelled}</DrawerTitle>
        {/* The sentence is the thing to tap to hear it: on a phone the
            speaker beside it is a small target, the sentence a large one. */}
        <Button
          onClick={() => void player.play(sentenceUrl)}
          variant="link"
          size={null}
          className="inline text-left text-xl leading-snug font-normal whitespace-normal text-dim not-data-disabled:hover:no-underline"
          title="Hear the sentence"
        >
          {marked.before}
          <strong className="font-semibold text-text">{marked.word}</strong>
          {marked.after}
        </Button>
      </DrawerHeader>
      <DrawerBody className="flex flex-col gap-6 pb-6">
        {token.name ? (
          <p className="text-lg text-dim">A name: left out of how much of the text is yours.</p>
        ) : token.parts.length === 0 ? (
          <p className="text-lg text-dim">
            hilvan's ten thousand words do not have this one, so it counts as not yours yet and cannot go into the deck.
          </p>
        ) : (
          token.parts.map((part, index) => (
            <section key={index} className="flex flex-col gap-4">
              {part.length > 1 && <p className="text-base text-faint">This spelling is {part.length} words; the sentence says which.</p>}
              {part.map((lemma) => {
                const id = lemmas[lemma]
                if (id === undefined) return null
                return (
                  <Settled key={id} fallback={<p className="text-base text-faint">Out of the tutor's reach - the word opens at home.</p>}>
                    <Suspense fallback={<p className="text-base text-faint">Looking it up…</p>}>
                      <Lemma
                        word={remembered(words, id, api.word)}
                        hold={holds[lemma] ?? null}
                        tapped={tapped}
                        declared={declaredHere.has(id)}
                        picked={pickedHere.has(pickKey(id, line.sentence))}
                        actions={actions}
                      />
                    </Suspense>
                  </Settled>
                )
              })}
            </section>
          ))
        )}
      </DrawerBody>
    </>
  )
}

/** Where the learner stands with a word, in words. */
function holdLine(hold: Hold | null): string {
  switch (hold) {
    case 'sewn':
      return 'Yours - holding'
    case 'basted':
      return 'Yours - being learnt'
    case 'deck':
      return 'In the deck - it opens among the first new words'
    default:
      return 'Not yours yet'
  }
}

function Lemma({
  word: promised,
  hold,
  tapped,
  declared,
  picked,
  actions,
}: {
  word: Promise<Word>
  hold: Hold | null
  tapped: Tapped
  declared: boolean
  picked: boolean
  actions: WordActions
}) {
  const word = use(promised)
  const level = commonness(word)
  const wordUrl = speechUrl(word.languages.target, word.lemma, 'slow')
  // Taken from this very sentence before - on another evening - counts as
  // taken here: it can be put back.
  const takenHere = picked || word.contexts.some((context) => context.sentence === tapped.line.sentence && context.source.kind === 'text')

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <span className="text-3xl font-semibold tracking-tight">{word.lemma}</span>
        {word.ipa && <span className="font-mono text-lg text-dim">/{word.ipa}/</span>}
        <Speak url={wordUrl} label={`Hear "${word.lemma}"`} className="ml-auto" />
      </div>
      {word.gloss ? <p className="text-xl text-accent">{word.gloss}</p> : <p className="text-base text-faint">No meaning in the pack for this one.</p>}
      <p className="text-base text-faint">
        {holdLine(hold)}
        {level && ` · ${level}`}
      </p>
      <div className="flex flex-wrap gap-2">
        {hold === null && (
          <>
            <Button onClick={() => actions.declare(word.id)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
              I know it
            </Button>
            <Button onClick={() => actions.pick(word.id, tapped)} variant="primary" size={null} className="px-4 py-3 text-lg">
              Into the deck
            </Button>
          </>
        )}
        {declared && hold === 'sewn' && (
          <Button onClick={() => actions.takeBack(word.id)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
            Take back "I know it"
          </Button>
        )}
        {takenHere && hold !== null && (
          <Button onClick={() => actions.unpick(word.id, tapped)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
            Out of the deck
          </Button>
        )}
        {!takenHere && hold !== null && (
          // A word already in hand or in the deck gains this sentence as one
          // more it is met in: the context the reader just understood it in.
          <Button onClick={() => actions.pick(word.id, tapped)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
            Learn it with this sentence too
          </Button>
        )}
      </div>
      <Family family={word.family} />
      <Settled fallback={null}>
        <Suspense fallback={null}>
          <Elsewhere examples={remembered(examples, word.id, api.examples)} besides={tapped.line.sentence} />
        </Suspense>
      </Settled>
    </div>
  )
}

/** Other sentences of the learner's own material the word stands in. */
function Elsewhere({ examples: promised, besides }: { examples: Promise<Example[]>; besides: number }) {
  const shown = use(promised).filter((example) => example.sentence !== besides)
  if (shown.length === 0) return null
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-xs tracking-caption text-faint uppercase">Where else you have it</h3>
      <ul className="flex flex-col gap-3">
        {shown.map((example) => {
          const chars = Array.from(example.text)
          return (
            <li key={example.sentence} className="flex flex-col gap-0.5">
              <span className="text-lg">
                {chars.slice(0, example.start).join('')}
                <strong className="font-semibold">{chars.slice(example.start, example.start + example.length).join('')}</strong>
                {chars.slice(example.start + example.length).join('')}
              </span>
              {example.translation && <span className="text-base text-dim">{example.translation}</span>}
              <span className="text-sm text-faint">
                {example.source.kind === 'formula' ? 'from a formula you drill' : example.source.title ? `from "${example.source.title}"` : 'from a text'}
              </span>
            </li>
          )
        })}
      </ul>
    </section>
  )
}

/** What could not be fetched shows its fallback, and the rest of the sheet stays. */
class Settled extends Component<{ children: ReactNode; fallback: ReactNode }, { failed: boolean }> {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children
  }
}
