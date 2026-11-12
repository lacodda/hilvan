import { cn } from 'dowel-ui'
import { Fragment, use, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'

import { api, speechUrl, textPath, Unreachable, type ChapterSummary, type Holds, type Standing, type Text } from '@/api'
import { Shell } from '@/App'
import { checker } from '@/core'
import { holdReads } from '@/offline'
import { chapterName, coverageLine, offsets, paragraphAt, placeIn, restOf, runs, swipe, throughText, tokensOf, withHolds, yours } from '@/reading'

import { Button } from '@/components/ui/button'
import { Drawer, DrawerBody, DrawerHeader, DrawerPopup, DrawerTitle } from '@/components/ui/drawer'
import { Track } from '@/components/ui/track'
import { useToastManager } from '@/components/ui/toast'
import { player, usePlaying, useStopOnLeave } from '@/speech'
import { forget, pickKey, WordSheet, type Tapped, type WordActions } from '@/WordSheet'

/** How long the place in a text waits before it is told to the tutor: a scroll is many moves, and one report. */
const PLACE_AFTER_MS = 1_500

/** How a running word is drawn, by how it stands. */
function tokenClass(standing: Standing | undefined): string {
  switch (standing) {
    case 'unknown':
      // Not yours: underlined, so the eye finds what to tap.
      return 'underline decoration-accent/70 decoration-dotted decoration-2 underline-offset-[5px]'
    case 'deck':
      return 'rounded-xs bg-accent-soft'
    case 'beyond':
      return 'text-dim underline decoration-line-2 decoration-dotted underline-offset-[5px]'
    default:
      return ''
  }
}

/** A book's chapters, each with how much of it is the learner's: where to go in it. */
function Contents({
  open,
  contents,
  shown,
  onChoose,
  onClose,
}: {
  open: boolean
  contents: readonly ChapterSummary[]
  shown: number
  onChoose: (chapter: number) => void
  onClose: () => void
}) {
  return (
    <Drawer open={open} onOpenChange={(next) => !next && onClose()}>
      <DrawerPopup side="bottom" size="lg" className="mx-auto max-w-xl">
        <DrawerHeader>
          <DrawerTitle>Chapters</DrawerTitle>
        </DrawerHeader>
        <DrawerBody className="pb-6">
          <ol className="flex flex-col">
            {contents.map((chapter) => {
              const share = yours(chapter.coverage) ?? 0
              return (
                <li key={chapter.position}>
                  <Button
                    onClick={() => onChoose(chapter.position)}
                    variant="ghost"
                    size={null}
                    aria-current={chapter.position === shown ? 'true' : undefined}
                    className="flex w-full flex-col items-stretch gap-1.5 px-3 py-3 text-left font-normal whitespace-normal text-text aria-[current=true]:bg-raise"
                  >
                    <span className="flex items-baseline justify-between gap-3">
                      <span className="text-lg text-balance">{chapterName(chapter)}</span>
                      <span className="shrink-0 text-base text-dim">{chapter.position === shown ? `Reading · ${share}%` : `${share}%`}</span>
                    </span>
                    <Track
                      segments={[{ key: 'yours', start: 0, end: share, tone: 'accent', label: `${share}% yours` }]}
                      from={0}
                      to={100}
                      minWidth={0}
                      size="sm"
                      label={coverageLine(chapter.coverage)}
                    />
                  </Button>
                </li>
              )
            })}
          </ol>
        </DrawerBody>
      </DrawerPopup>
    </Drawer>
  )
}

/**
 * Reading a text, a chapter at a time.
 *
 * The text as paragraphs, every word it holds drawn by how it stands with
 * the learner: plain when it is theirs, underlined when it is not, lit when
 * it waits in the deck. A tap on a word opens it; a tap between words says
 * the sentence; a swipe to the right across a paragraph says every word in
 * it not yet in hand is known - the button at the bottom does the same for
 * the paragraph being read. How much is theirs is counted again after every
 * change, on this device, by the code the tutor counts the list with.
 *
 * A book is read a chapter at a time - a phone holds a chapter, not a novel
 * - with its chapters a tap away, each saying how much of it is the
 * learner's. The place is kept through the whole book.
 *
 * The page itself - the measure, the size, the place kept by paragraph - is
 * rhapsod's reader, copied until the line's Reader block is in dowel.
 */
export function ReaderScreen({
  text: promised,
  onChapter,
  onLeave,
}: {
  text: Promise<Text>
  onChapter: (chapter: number) => void
  onLeave: () => void
}) {
  const text = use(promised)
  const counter = use(checker())
  const chapter = text.contents[text.shown] ?? text.chapter
  const book = text.chapters > 1
  const last = text.shown >= text.contents.length - 1
  // Where the learner stopped, in this chapter's own paragraphs; null when
  // they stopped in another.
  const placeHere = placeIn(text)
  const [contentsOpen, setContentsOpen] = useState(false)
  const [holds, setHolds] = useState(text.holds)
  const [tapped, setTapped] = useState<Tapped | null>(null)
  const [declaredHere, setDeclaredHere] = useState<ReadonlySet<string>>(new Set())
  const [pickedHere, setPickedHere] = useState<ReadonlySet<string>>(new Set())
  const [finished, setFinished] = useState(text.finished_at !== null)
  const [at, setAt] = useState(placeHere ?? 0)
  const [swept, setSwept] = useState<number | null>(null)
  const toasts = useToastManager()
  const playing = usePlaying()
  useStopOnLeave()

  // The chapter after this one, kept for the road while the tutor is in
  // reach: a chapter finished on a train opens the next one there.
  const following = text.contents[text.shown + 1]?.position
  useEffect(() => {
    if (following !== undefined) void holdReads([`/api${textPath(text.id, following)}`])
  }, [text.id, following])

  const tokens = useMemo(() => tokensOf(text), [text])
  const starts = useMemo(() => offsets(text.lines), [text])
  const { coverage, standings } = useMemo(() => counter.cover(tokens, holds), [counter, tokens, holds])

  const changed = (after: Holds) => setHolds((before) => withHolds(text.lemmas, before, after))
  const failed = (what: string) => (error: unknown) =>
    toasts.add({
      title: what,
      description: error instanceof Unreachable ? 'Out of the tutor’s reach - try again at home.' : error instanceof Error ? error.message : String(error),
      type: 'error',
    })

  const takeBack = (words: string[]) => {
    void api
      .takeBack(words)
      .then(({ taken, holds: after }) => {
        changed(after)
        setDeclaredHere((before) => new Set([...before].filter((word) => !taken.includes(word))))
      })
      .catch(failed('Not taken back'))
  }

  const declare = (words: string[], said: string) => {
    void api
      .declare(words)
      .then(({ declared, holds: after }) => {
        changed(after)
        setDeclaredHere((before) => new Set([...before, ...declared]))
        if (declared.length === 0) return
        toasts.add({
          title: said,
          description: declared.length === 1 ? 'Sewn on your word; a sentence will check it.' : `${declared.length} words sewn on your word; sentences will check them.`,
          timeout: 8_000,
          actionProps: { children: 'Undo', onClick: () => takeBack(declared) },
        })
      })
      .catch(failed('Not saved'))
  }

  const actions: WordActions = {
    declare: (word) => declare([word], 'Known'),
    takeBack: (word) => takeBack([word]),
    pick: (word, { line, token }) => {
      void api
        .pick(word, line.sentence, token.start)
        .then(({ holds: after }) => {
          forget(word)
          changed(after)
          setPickedHere((before) => new Set([...before, pickKey(word, line.sentence)]))
        })
        .catch(failed('Not put in the deck'))
    },
    unpick: (word, { line }) => {
      void api
        .unpick(word, line.sentence)
        .then(({ holds: after }) => {
          forget(word)
          changed(after)
          setPickedHere((before) => new Set([...before].filter((key) => key !== pickKey(word, line.sentence))))
        })
        .catch(failed('Not taken out'))
    },
  }

  const knowTheRest = (paragraph: number) => {
    const lines = text.lines[paragraph]
    if (!lines) return
    const words = restOf(lines, text.lemmas, holds)
    if (words.length === 0) {
      toasts.add({ title: 'Nothing left here', description: 'Every word of this paragraph is yours or in the deck.', timeout: 4_000 })
      return
    }
    setSwept(paragraph)
    declare(words, 'The rest is known')
  }

  // The flash on a swept paragraph, gone after a moment.
  useEffect(() => {
    if (swept === null) return undefined
    const timer = window.setTimeout(() => setSwept(null), 700)
    return () => window.clearTimeout(timer)
  }, [swept])

  // Back where the learner stopped, or at the top of a chapter opened.
  // Once: the text is here before the first paint, and a paragraph flush
  // against the top reads as a page cut.
  const paragraphs = useRef<(HTMLParagraphElement | null)[]>([])
  useEffect(() => {
    if (placeHere === null || placeHere <= 0) {
      window.scrollTo({ top: 0 })
      return
    }
    paragraphs.current[placeHere]?.scrollIntoView({ block: 'start' })
    window.scrollBy({ top: -72 })
  }, [placeHere])

  // Follow the reader down the page.
  useEffect(() => {
    let frame = 0
    const onScroll = () => {
      if (frame) return
      frame = window.requestAnimationFrame(() => {
        frame = 0
        const tops = paragraphs.current.map((element) => (element ? element.getBoundingClientRect().top : null))
        setAt(paragraphAt(tops, window.innerHeight / 2))
      })
    }
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      window.removeEventListener('scroll', onScroll)
      if (frame) window.cancelAnimationFrame(frame)
    }
  }, [])

  // The place told to the tutor once the reader settles on it, and on the
  // way out. Opening the text is telling it too: the list orders by it.
  const told = useRef<number | null>(null)
  useEffect(() => {
    if (told.current === at) return undefined
    const timer = window.setTimeout(
      () => {
        told.current = at
        // Through the whole text: a chapter's paragraphs start where it does.
        void api.placeInText(text.id, { paragraph: throughText(chapter, at) }).catch(() => undefined)
      },
      told.current === null ? 0 : PLACE_AFTER_MS,
    )
    return () => window.clearTimeout(timer)
  }, [at, text.id, chapter])

  const finish = (read: boolean) => {
    setFinished(read)
    void api.placeInText(text.id, { finished: read }).catch(failed('Not saved'))
  }

  // The swipe: watched from a finger's first touch on a paragraph.
  const drag = useRef<{ x: number; y: number; paragraph: number } | null>(null)
  const onDown = (paragraph: number) => (event: ReactPointerEvent) => {
    drag.current = event.pointerType === 'touch' ? { x: event.clientX, y: event.clientY, paragraph } : null
  }
  const onMove = (event: ReactPointerEvent) => {
    const start = drag.current
    if (!start) return
    const verdict = swipe(start, event.clientX, event.clientY)
    if (verdict === 'watching') return
    drag.current = null
    if (verdict === 'known') knowTheRest(start.paragraph)
  }

  const segments = [
    { key: 'sewn', start: 0, end: coverage.sewn, tone: 'accent' as const, label: `${coverage.sewn} held` },
    { key: 'basted', start: coverage.sewn, end: coverage.sewn + coverage.basted, tone: 'past' as const, label: `${coverage.basted} being learnt` },
    {
      key: 'deck',
      start: coverage.sewn + coverage.basted,
      end: coverage.sewn + coverage.basted + coverage.deck,
      tone: 'good' as const,
      label: `${coverage.deck} in the deck`,
    },
  ]

  return (
    <Shell
      aside={
        <Button onClick={onLeave} variant="link" className="text-base text-faint underline">
          Your texts
        </Button>
      }
      actions={
        <div className="flex items-center gap-3">
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <span className="text-base text-dim">{book ? `This chapter: ${coverageLine(coverage)}` : coverageLine(coverage)}</span>
            <Track segments={segments} from={0} to={Math.max(coverage.words, 1)} minWidth={0} size="sm" label={coverageLine(coverage)} />
          </div>
          <Button onClick={() => knowTheRest(at)} variant="ghost" size={null} className="px-3 py-2.5 text-base whitespace-normal text-text">
            I know the rest
          </Button>
        </div>
      }
    >
      <header className="flex flex-col gap-1">
        <h2 className="text-2xl font-semibold tracking-tight text-balance">{text.title}</h2>
        {text.author && <p className="text-base text-faint">{text.author}</p>}
        {book && (
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span className="text-lg text-dim">
              Chapter {text.shown + 1} of {text.chapters}
              {chapter.title ? ` · ${chapter.title}` : ''}
            </span>
            <Button onClick={() => setContentsOpen(true)} variant="link" className="text-base text-faint underline">
              Chapters
            </Button>
          </div>
        )}
        <p className="text-base text-faint">Tap a word you do not know; tap between words to hear the sentence; swipe a paragraph to the right when you know the rest of it.</p>
      </header>

      <article className="flex flex-col gap-5">
        {text.lines.map((lines, paragraph) => (
          <p
            key={paragraph}
            ref={(element) => {
              paragraphs.current[paragraph] = element
            }}
            onPointerDown={onDown(paragraph)}
            onPointerMove={onMove}
            onPointerUp={() => (drag.current = null)}
            onPointerCancel={() => (drag.current = null)}
            className={`touch-pan-y text-xl leading-[1.75] text-pretty text-text transition-colors duration-500 sm:text-lg sm:leading-[1.8] ${
              swept === paragraph ? 'bg-accent-soft' : ''
            }`}
          >
            {lines.map((line, index) => {
              const url = speechUrl(text.language, line.text, 'normal')
              const first = starts[paragraph]?.[index] ?? 0
              return (
                <Fragment key={line.sentence}>
                  <span
                    onClick={(event) => {
                      // A tap between words says the sentence; a tap on one opens it.
                      if ((event.target as HTMLElement).closest('button')) return
                      void player.play(url)
                    }}
                    className={playing === url ? 'rounded-xs bg-raise' : ''}
                  >
                    {runs(line).map((run, piece) =>
                      run.token === null ? (
                        <Fragment key={piece}>{run.text}</Fragment>
                      ) : (
                        <Button
                          key={piece}
                          onClick={() => setTapped({ line, token: line.tokens[run.token!]! })}
                          variant="link"
                          size={null}
                          className={cn(
                            // A word of the text, not a control on it: the
                            // paragraph's own size, weight and colour.
                            'inline cursor-pointer align-baseline font-normal whitespace-normal text-text not-data-disabled:hover:no-underline',
                            tokenClass(standings[first + run.token]),
                          )}
                        >
                          {run.text}
                        </Button>
                      ),
                    )}
                  </span>{' '}
                </Fragment>
              )
            })}
          </p>
        ))}
      </article>

      <section className="flex flex-col gap-3 border-t border-line pt-6">
        {book && !last ? (
          <Button onClick={() => onChapter(text.shown + 1)} variant="primary" size={null} className="flex-col items-start self-start px-4 py-3 text-left whitespace-normal">
            <span className="block text-lg">Next chapter</span>
            {text.contents[text.shown + 1] && <span className="block text-base opacity-80">{chapterName(text.contents[text.shown + 1]!)}</span>}
          </Button>
        ) : (
          <Button onClick={() => finish(!finished)} variant={finished ? 'ghost' : 'primary'} size={null} className="self-start px-4 py-3 text-lg">
            {finished ? 'Read · mark unread' : book ? 'I have read the book' : 'I have read it'}
          </Button>
        )}
        {book && text.shown > 0 && (
          <Button onClick={() => onChapter(text.shown - 1)} variant="link" className="self-start text-base text-faint underline">
            Previous chapter
          </Button>
        )}
      </section>

      {book && (
        <Contents
          open={contentsOpen}
          contents={text.contents}
          shown={text.shown}
          onChoose={(position) => {
            setContentsOpen(false)
            if (position !== text.shown) onChapter(position)
          }}
          onClose={() => setContentsOpen(false)}
        />
      )}

      <WordSheet
        tapped={tapped}
        lemmas={text.lemmas}
        holds={holds}
        language={text.language}
        declaredHere={declaredHere}
        pickedHere={pickedHere}
        actions={actions}
        onClose={() => setTapped(null)}
      />
    </Shell>
  )
}
