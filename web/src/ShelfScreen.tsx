import { use, useState } from 'react'

import { api, type ShelfItem } from '@/api'
import { Shell } from '@/App'
import { fit, levelLine, shelfOrder, WINDOW } from '@/reading'
import { Button } from '@/components/ui/button'

/** How a shelf item stands to the window, in a word or two beside its share. */
function fitNote(estimate: number): string {
  switch (fit(estimate)) {
    case 'inside':
      return 'your level'
    case 'easier':
      return 'easy'
    case 'harder':
      return estimate >= WINDOW.low - 5 ? 'a stretch' : 'hard for now'
  }
}

function Item({ item, busy, onTake, onOpen }: { item: ShelfItem; busy: boolean; onTake: () => void; onOpen: (id: number) => void }) {
  const length = item.kind === 'book' ? `${Math.round(item.words / 1000).toLocaleString('en')}k words` : `${item.words.toLocaleString('en')} words`
  return (
    <li className="flex flex-col gap-2 rounded-md border border-line p-4">
      <div className="flex flex-col">
        <span className="text-xl font-medium text-balance">{item.title}</span>
        {item.author && <span className="text-base text-faint">{item.author}</span>}
      </div>
      <span className="text-base text-dim">
        About {item.estimate}% yours · {fitNote(item.estimate)}
      </span>
      <span className="text-base text-faint">
        {length} · needs {levelLine(item.level)} for 95% of it
      </span>
      {item.taken === null ? (
        <Button onClick={onTake} disabled={busy} variant="ghost" size={null} className="self-start px-3 py-2 text-base text-text">
          {busy ? 'Fetching it…' : 'Take it'}
        </Button>
      ) : (
        <Button onClick={() => item.taken !== null && onOpen(item.taken)} variant="link" className="self-start text-base text-text underline">
          On your list - open it
        </Button>
      )}
    </li>
  )
}

/**
 * The shelf: open books and articles, each with about how much of it is
 * the learner's - counted from what they hold of each band of the
 * language's frequency list, before the text is fetched. Taking one brings
 * it to the learner's texts, where it is counted word by word.
 */
export function ShelfScreen({ items: promised, onOpen, onLeave }: { items: Promise<ShelfItem[]>; onOpen: (id: number) => void; onLeave: () => void }) {
  const items = shelfOrder(use(promised))
  const [taking, setTaking] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  const take = (item: ShelfItem) => {
    setTaking(item.id)
    setProblem(null)
    api
      .takeFromShelf(item.id)
      .then((added) => onOpen(added.id))
      .catch((error: unknown) => setProblem(`${item.title}: ${error instanceof Error ? error.message : String(error)}`))
      .finally(() => setTaking(null))
  }

  const section = (kind: ShelfItem['kind'], heading: string, about: string) => {
    const of = items.filter((item) => item.kind === kind)
    if (of.length === 0) return null
    return (
      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <h3 className="text-base font-medium tracking-caption text-dim uppercase">{heading}</h3>
          <p className="text-base text-faint">{about}</p>
        </div>
        <ul className="flex flex-col gap-2">
          {of.map((item) => (
            <Item key={item.id} item={item} busy={taking === item.id} onTake={() => take(item)} onOpen={onOpen} />
          ))}
        </ul>
      </section>
    )
  }

  return (
    <Shell
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Your texts
        </Button>
      }
    >
      <section className="flex flex-col gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">The shelf</h2>
        <p className="text-lg text-dim">
          Open books and articles, the ones most yours first. A text reads on its own when {WINDOW.low}% to {WINDOW.high}% of its
          words are yours; until then, the nearest is the place to take words from.
        </p>
      </section>
      {problem && <p className="text-base text-bad">{problem}</p>}
      {section('article', 'Articles', 'The Simple English Wikipedia: short, and written for readers still learning.')}
      {section('book', 'Books', 'Project Gutenberg: in chapters, each counted on its own once taken.')}
    </Shell>
  )
}
