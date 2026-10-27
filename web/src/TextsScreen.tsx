import { use, useState, type ChangeEvent } from 'react'

import { api, type TextFormat, type TextSummary } from '@/api'
import { Shell } from '@/App'
import { ACCEPTED, coverageLine, formatOf, placeLine, titleOf, yours } from '@/reading'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Track } from '@/components/ui/track'

/** A text being added: what was pasted or read from a file. */
interface Draft {
  title: string
  body: string
  /** Said by the file's name; null for a paste, which the tutor tells by itself. */
  format: TextFormat | null
}

const EMPTY: Draft = { title: '', body: '', format: null }

/**
 * Your texts: what you read, how much of each is yours, and where you are
 * in it - and the way to add another, pasted or from a file.
 */
export function TextsScreen({
  texts,
  onOpen,
  onLeave,
}: {
  texts: Promise<TextSummary[]>
  onOpen: (id: number) => void
  onLeave: () => void
}) {
  const [list, setList] = useState(use(texts))
  const [adding, setAdding] = useState(list.length === 0)
  const [draft, setDraft] = useState<Draft>(EMPTY)
  const [saving, setSaving] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const [removing, setRemoving] = useState<number | null>(null)

  const fromFile = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (!file) return
    void file.text().then((body) =>
      setDraft((before) => ({ title: before.title.trim() ? before.title : titleOf(file.name), body, format: formatOf(file.name) })),
    )
  }

  const add = () => {
    if (!draft.body.trim()) {
      setProblem('Paste a text, or choose a file.')
      return
    }
    setSaving(true)
    setProblem(null)
    api
      .addText(draft.body, draft.title.trim() || null, draft.format)
      .then((added) => {
        setDraft(EMPTY)
        onOpen(added.id)
      })
      .catch((error: unknown) => setProblem(error instanceof Error ? error.message : String(error)))
      .finally(() => setSaving(false))
  }

  const remove = (id: number) => {
    void api
      .removeText(id)
      .then(() => setList((before) => before.filter((text) => text.id !== id)))
      .catch((error: unknown) => setProblem(error instanceof Error ? error.message : String(error)))
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
          Anything you want to read in English: an article, a page of a book, the subtitles of an episode. Each says how much
          of it is already yours, and a word you do not know goes into the deck with the sentence you met it in.
        </p>
      </section>

      {adding ? (
        <section className="flex flex-col gap-3 rounded-md bg-raise p-4">
          <h3 className="text-base font-medium tracking-caption text-dim uppercase">A new text</h3>
          <Input
            value={draft.title}
            onChange={(event) => setDraft({ ...draft, title: event.target.value })}
            placeholder="Title - or it is taken from the text"
            aria-label="Title"
          />
          <Textarea
            value={draft.body}
            onChange={(event) => setDraft({ ...draft, body: event.target.value, format: null })}
            placeholder="Paste the text here: plain, markdown or subtitles"
            aria-label="The text"
            autoResize
            maxRows={14}
            className="min-h-40"
          />
          <label className="flex flex-col gap-1 text-base text-dim">
            Or a file - .txt, .md, .srt or .vtt
            <input type="file" accept={ACCEPTED} onChange={fromFile} className="text-base text-dim file:mr-3 file:rounded-md file:border file:border-line file:bg-bg file:px-3 file:py-2 file:text-text" />
          </label>
          {problem && <p className="text-base text-bad">{problem}</p>}
          <div className="flex flex-wrap gap-2">
            <Button onClick={add} disabled={saving} variant="primary" size={null} className="px-4 py-3 text-lg">
              {saving ? 'Reading it…' : 'Add and read'}
            </Button>
            {list.length > 0 && (
              <Button onClick={() => setAdding(false)} variant="ghost" size={null} className="px-4 py-3 text-lg text-text">
                Not now
              </Button>
            )}
          </div>
        </section>
      ) : (
        <Button onClick={() => setAdding(true)} variant="ghost" size={null} className="flex-col items-start px-4 py-3 text-left whitespace-normal text-text">
          <span className="block text-xl font-medium">Add a text</span>
          <span className="block text-base text-faint">Paste it, or open a file.</span>
        </Button>
      )}

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
                  <span className="text-xl font-medium text-balance">{text.title}</span>
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
    </Shell>
  )
}
