/**
 * The reader's arithmetic, apart from the screen that draws it: a sentence
 * cut into its running words, the words of a paragraph still to be said
 * known, how much of a text is the learner's, and what a file is.
 */

import type { Coverage, Hold, Holds, Line, Text, TextFormat, TextSummary, Token } from '@/api'

/** A stretch of a sentence: a running word (by its place among the sentence's tokens) or what stands between words. */
export interface Run {
  text: string
  /** The token's index in its line, or null for spaces and punctuation. */
  token: number | null
}

/**
 * A sentence cut into runs at its words.
 *
 * The server counts in code points and a JavaScript string indexes by UTF-16
 * unit, which part company at the first character outside the basic plane:
 * hence `Array.from`. A token that does not fit where it is said to be is
 * left as plain text rather than drawn over the wrong letters.
 */
export function runs(line: Pick<Line, 'text' | 'tokens'>): Run[] {
  const chars = Array.from(line.text)
  const cut: Run[] = []
  let at = 0
  line.tokens.forEach((token, index) => {
    if (token.start < at || token.start + token.length > chars.length) return
    if (token.start > at) cut.push({ text: chars.slice(at, token.start).join(''), token: null })
    cut.push({ text: chars.slice(token.start, token.start + token.length).join(''), token: index })
    at = token.start + token.length
  })
  if (at < chars.length) cut.push({ text: chars.slice(at).join(''), token: null })
  return cut
}

/** Where each line's tokens begin in the text's one list of standings, paragraph by paragraph. */
export function offsets(lines: readonly (readonly Pick<Line, 'tokens'>[])[]): number[][] {
  let at = 0
  return lines.map((paragraph) =>
    paragraph.map((line) => {
      const start = at
      at += line.tokens.length
      return start
    }),
  )
}

/** Every token of a text, in order: what the core counts. */
export function tokensOf(text: Pick<Text, 'lines'>): Token[] {
  return text.lines.flatMap((paragraph) => paragraph.flatMap((line) => line.tokens))
}

/**
 * The words a paragraph would have said known by "I know the rest": for
 * running word that is not a name, each part not in hand and not in the
 * deck, as its commonest reading. A word past the lexicon has no part to
 * declare.
 *
 * The commonest, because the lexicon lists a spelling's lemmas commonest
 * first and cannot tell which one the sentence means: "left" is far more
 * often "leave" than "left". A word put in the deck is the learner saying
 * they do not know it, and is left alone.
 */
export function restOf(paragraph: readonly Pick<Line, 'tokens'>[], lemmas: readonly string[], holds: readonly (Hold | null)[]): string[] {
  const words: string[] = []
  for (const line of paragraph) {
    for (const token of line.tokens) {
      if (token.name) continue
      for (const part of token.parts) {
        if (part.some((index) => holds[index] !== null && holds[index] !== undefined)) continue
        const first = part[0]
        const id = first === undefined ? undefined : lemmas[first]
        if (id !== undefined && !words.includes(id)) words.push(id)
      }
    }
  }
  return words
}

/** The text's holds with what a change says of some of its words. */
export function withHolds(lemmas: readonly string[], holds: readonly (Hold | null)[], changed: Holds): (Hold | null)[] {
  return lemmas.map((id, index) => (id in changed ? (changed[id] ?? null) : (holds[index] ?? null)))
}

/** How much of a text is the learner's, as a whole percent; null for a text with no words to count. */
export function yours(coverage: Coverage): number | null {
  if (coverage.words === 0) return null
  return Math.floor((100 * (coverage.sewn + coverage.basted)) / coverage.words)
}

/**
 * The coverage in a line: "86% yours". Rounded down, so a text one word
 * short of the whole of it never claims to be all the learner's.
 */
export function coverageLine(coverage: Coverage): string {
  const share = yours(coverage)
  if (share === null) return 'No words to count'
  const deck = coverage.deck > 0 ? `, ${coverage.deck} in the deck` : ''
  return `${share}% yours${deck}`
}

/** How far into a text the learner is: "paragraph 3 of 12", or "read". */
export function placeLine(summary: Pick<TextSummary, 'paragraph' | 'paragraphs' | 'finished_at' | 'opened_at'>): string {
  if (summary.finished_at !== null) return 'Read'
  if (summary.opened_at === null) return 'Not opened yet'
  return `Paragraph ${summary.paragraph + 1} of ${summary.paragraphs}`
}

/** What a file is, by its name; null to let the tutor tell from what is in it. */
export function formatOf(name: string): TextFormat | null {
  const extension = name.toLowerCase().split('.').at(-1)
  if (extension === 'md' || extension === 'markdown') return 'markdown'
  if (extension === 'srt' || extension === 'vtt') return 'subtitles'
  if (extension === 'txt') return 'plain'
  return null
}

/** A title from a file's name: the extension off, the separators spaces. */
export function titleOf(name: string): string {
  const stem = name.replace(/\.[^.]+$/, '')
  return stem.replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim()
}

/** The files the reader takes. */
export const ACCEPTED = '.txt,.md,.markdown,.srt,.vtt,text/plain,text/markdown'

/** How far a finger has to travel across a paragraph to say "I know the rest". */
export const SWIPE = 80

/** Beyond this much vertical movement it is a scroll, not a swipe. */
export const SLOPE = 30

/** What a movement to (x, y) means for a drag that began at `start`. */
export type Swipe = 'known' | 'abandon' | 'watching'

/**
 * The swipe across a paragraph, apart from the browser that delivers it.
 *
 * Rightwards only - the way a hand sweeps a page aside - and abandoned the
 * moment it runs down the page: a reader scrolling must never declare a
 * paragraph known, however far the scroll drifts sideways.
 */
export function swipe(start: { x: number; y: number }, x: number, y: number): Swipe {
  if (Math.abs(y - start.y) > SLOPE) return 'abandon'
  if (start.x - x > SLOPE) return 'abandon'
  if (x - start.x >= SWIPE) return 'known'
  return 'watching'
}

/**
 * The paragraph being read: the last one whose top has passed the middle
 * of the screen - the line the eye is on, not the one about to appear.
 */
export function paragraphAt(tops: readonly (number | null)[], middle: number): number {
  let at = 0
  tops.forEach((top, index) => {
    if (top !== null && top < middle) at = index
  })
  return at
}
