import { use, useEffect, useState, type ChangeEvent } from 'react'

import { ANKI_URL, api, textPath, type NextRead, type TextSummary, type Texts } from '@/api'
import { Shell } from '@/App'
import { holdReads } from '@/offline'
import { ACCEPTED, coverageLine, nextLine, nextReason, placeLine, yours } from '@/reading'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Track } from '@/components/ui/track'

/** A text being pasted. */
interface Draft {
  title: string
  body: string
}

const EMPTY: Draft = { title: '', body: '' }

/** What is being done to bring a text in, said while it is done. */
type Bringing = 'paste' | 'link' | 'file' | 'next' | null

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** What to read next, and why: the first thing on the list. */
function NextCard({ next, busy, onRead }: { next: NextRead; busy: boolean; onRead: () => void }) {
  const { title, detail } = nextLine(next)
  return (
    <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
      <h3 className="text-base font-medium tracking-caption text-dim uppercase">Read next</h3>
      <div className="flex flex-col gap-1">
        <span className="text-xl font-medium text-balance">{title}</span>
        <span className="text-base text-dim">{detail}</span>
      </div>
      <p className="text-base text-faint">{nextReason(next)}</p>
      <Button onClick={onRead} disabled={busy} variant="primary" size={null} className="self-start px-4 py-3 text-lg">
        {busy ? 'Fetching it…' : next.kind === 'shelf' ? 'Take it and read' : next.started ? 'Read on' : 'Read it'}
      </Button>
    </section>
  )
}

/**
 * Your texts: what you read, how much of each is yours, and where you are
 * in it - the one to read next first, and the ways to bring another: a
 * link, a book or a file, a paste, or the shelf.
 */
export function TextsScreen({
  texts,
  onOpen,
  onShelf,
  onLeave,
}: {
  texts: Promise<Texts>
  onOpen: (id: number) => void
  onShelf: () => void
  onLeave: () => void
}) {
  const read = use(texts)
  const [list, setList] = useState<TextSummary[]>(read.texts)
  const [adding, setAdding] = useState(read.texts.length === 0)
  const [draft, setDraft] = useState<Draft>(EMPTY)
  const [link, setLink] = useState('')
  const [bringing, setBringing] = useState<Bringing>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [removing, setRemoving] = useState<number | null>(null)

  // What to read next, kept for the road while the tutor is in reach: a
  // text of the learner's own opens where they stopped on a train too. One
  // from the shelf has to be fetched by the tutor first, and is kept as it
  // is opened.
  const nextText = read.next?.kind === 'text' ? read.next.id : null
  useEffect(() => {
    if (nextText !== null) void holdReads([`/api${textPath(nextText)}`])
  }, [nextText])

  const bring =(what: Exclude<Bringing, null>, arriving: () => Promise<TextSummary>) => {
    setBringing(what)
    setProblem(null)
    arriving()
      .then((added) => onOpen(added.id))
      .catch((error: unknown) => setProblem(message(error)))
      .finally(() => setBringing(null))
  }

  const paste = () => {
    if (!draft.body.trim()) {
      setProblem('Paste a text first.')
      return
    }
    bring('paste', () => api.addText(draft.body, draft.title.trim() || null, null))
  }

  const follow = () => {
    if (!link.trim()) {
      setProblem('Paste a link first.')
      return
    }
    bring('link', () => api.addLink(link.trim()))
  }

  const fromFile = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (!file) return
    bring('file', () => api.addFile(file, file.name))
  }

  const readNext = (next: NextRead) => {
    if (next.kind === 'text') {
      onOpen(next.id)
      return
    }
    bring('next', () => api.takeFromShelf(next.id))
  }

  const remove = (id: number) => {
    void api
      .removeText(id)
      .then(() => setList((before) => before.filter((text) => text.id !== id)))
      .catch((error: unknown) => setProblem(message(error)))
      .finally(() => setRemoving(null))
  }

  return (
    <Shell
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Today
        </Button>
      }
    >
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Your texts</h2>
        <p className="text-lg text-dim">
          Books, articles, the subtitles of an episode: each says how much of it is already yours, and a word you do not know
          goes into the deck with the sentence you met it in.
        </p>
      </section>

      {read.next && <NextCard next={read.next} busy={bringing === 'next'} onRead={() => read.next && readNext(read.next)} />}

      <Button onClick={onShelf} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
        <span className="block text-xl font-medium">The shelf</span>
        <span className="block text-base text-faint">Open books and articles, by how much of each is yours.</span>
      </Button>

      {adding ? (
        <section className="flex flex-col gap-5 rounded-md bg-raise p-4">
          <h3 className="text-base font-medium tracking-caption text-dim uppercase">A new text</h3>

          <div className="flex flex-col gap-2">
            <label htmlFor="link" className="text-base text-dim">
              By a link - a book at Project Gutenberg, a Wikipedia article, any page
            </label>
            <div className="flex gap-2">
              <Input
                id="link"
                type="url"
                inputMode="url"
                value={link}
                onChange={(event) => setLink(event.target.value)}
                onKeyDown={(event) => event.key === 'Enter' && follow()}
                placeholder="https://"
                className="min-w-0 flex-1"
              />
              <Button onClick={follow} disabled={bringing !== null} variant="primary" size={null} className="px-4 py-2 text-lg">
                {bringing === 'link' ? 'Fetching…' : 'Add'}
              </Button>
            </div>
          </div>

          <label className="flex flex-col gap-1 text-base text-dim">
            A book or a file - .epub, .fb2, .txt, .md, .srt, .vtt, .html
            <input
              type="file"
              accept={ACCEPTED}
              onChange={fromFile}
              disabled={bringing !== null}
              className="text-base text-dim file:mr-3 file:rounded-md file:border file:border-line file:bg-bg file:px-3 file:py-2 file:text-text"
            />
            {bringing === 'file' && <span className="text-faint">Reading it…</span>}
          </label>

          <div className="flex flex-col gap-2">
            <span className="text-base text-dim">Or paste it</span>
            <Input value={draft.title} onChange={(event) => setDraft({ ...draft, title: event.target.value })} placeholder="Title - or it is taken from the text" aria-label="Title" />
            <Textarea
              value={draft.body}
              onChange={(event) => setDraft({ ...draft, body: event.target.value })}
              placeholder="Paste the text here: plain, markdown or subtitles"
              aria-label="The text"
              autoResize
              maxRows={14}
              className="min-h-32"
            />
            <div className="flex flex-wrap gap-2">
              <Button onClick={paste} disabled={bringing !== null} variant="primary" size={null} className="px-4 py-3 text-lg">
                {bringing === 'paste' ? 'Reading it…' : 'Add and read'}
              </Button>
              {list.length > 0 && (
                <Button onClick={() => setAdding(false)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
                  Not now
                </Button>
              )}
            </div>
          </div>
          {problem && <p className="text-base text-bad">{problem}</p>}
        </section>
      ) : (
        <Button onClick={() => setAdding(true)} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
          <span className="block text-xl font-medium">Add a text</span>
          <span className="block text-base text-faint">By a link, from a book or a file, or pasted.</span>
        </Button>
      )}
      {!adding && problem && <p className="text-base text-bad">{problem}</p>}

      {list.length > 0 && (
        <ul className="flex flex-col gap-2">
          {list.map((text) => {
            const share = yours(text.coverage)
            return (
              <li key={text.id} className="flex flex-col gap-2 rounded-md border border-line p-4">
                <Button
                  onClick={() => onOpen(text.id)}
                  variant="link"
                  size={null}
                  className="flex flex-col items-stretch gap-2 text-left font-normal whitespace-normal text-text not-data-disabled:hover:no-underline"
                >
                  <span className="flex flex-col">
                    <span className="text-xl font-medium text-balance">{text.title}</span>
                    {text.author && <span className="text-base text-faint">{text.author}</span>}
                  </span>
                  <span className="text-base text-dim">
                    {coverageLine(text.coverage)} · {placeLine(text)}
                  </span>
                  <Track
                    segments={[
                      { key: 'yours', start: 0, end: share ?? 0, tone: 'accent', label: `${share ?? 0}% yours` },
                      {
                        key: 'deck',
                        start: share ?? 0,
                        end: (share ?? 0) + (text.coverage.words ? (100 * text.coverage.deck) / text.coverage.words : 0),
                        tone: 'good',
                        label: `${text.coverage.deck} in the deck`,
                      },
                    ]}
                    from={0}
                    to={100}
                    minWidth={0}
                    size="sm"
                    label={coverageLine(text.coverage)}
                  />
                </Button>
                {removing === text.id ? (
                  <div className="flex flex-wrap items-center gap-3 text-base">
                    <span className="text-dim">Let it go? The words you took from it stay in the deck.</span>
                    <Button onClick={() => remove(text.id)} variant="danger" size={null} className="px-3 py-2 text-base">
                      Let it go
                    </Button>
                    <Button onClick={() => setRemoving(null)} variant="link" className="text-base text-faint underline">
                      Keep it
                    </Button>
                  </div>
                ) : (
                  <Button onClick={() => setRemoving(text.id)} variant="link" className="self-start text-base text-faint underline">
                    Remove
                  </Button>
                )}
              </li>
            )
          })}
        </ul>
      )}

      <section className="flex flex-col gap-1 border-t border-line pt-6">
        <a href={ANKI_URL} download className="self-start text-base text-dim underline">
          The words you took, for Anki
        </a>
        <span className="text-base text-faint">A file Anki and AnkiDroid import: each word in the sentence you took it from. Importing it again updates the cards.</span>
      </section>
    </Shell>
  )
}
