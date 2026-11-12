import type { Kin } from '@/api'
import { kinHold } from '@/words'

/**
 * The words of a word's family - decision and decisive beside decide - each
 * with what it means and whether it is already the learner's. Shown with the
 * word, so a family is learnt together: one of its members known is most of
 * the way to the others.
 */
export function Family({ family }: { family: readonly Kin[] }) {
  if (family.length === 0) return null
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-sm font-medium tracking-caption text-dim uppercase">Its family</h3>
      <ul className="flex flex-wrap gap-2">
        {family.map((kin) => (
          <li key={kin.id} className="flex flex-col rounded-md bg-raise px-3 py-2">
            <span className="text-lg font-medium">{kin.lemma}</span>
            {kin.gloss && <span className="text-base text-dim">{kin.gloss}</span>}
            {kinHold(kin.hold) && <span className="text-xs text-faint">{kinHold(kin.hold)}</span>}
          </li>
        ))}
      </ul>
    </section>
  )
}
