import { Track } from '@/components/ui/track'
import { holdLine, scale, segments } from '@/holds'

/**
 * How firmly a card holds: a bar on a scale of hours to a year, and the
 * same said in words. Given `was`, the stability before an answer, the bar
 * shows how far the answer moved it.
 *
 * No marker: the end of the fill is the place, and the hairlines at the
 * bands' edges say how far along the scale that is.
 */
export function Hold({ stability, was }: { stability: number; was?: number }) {
  const line = holdLine(stability, was)
  return (
    <div className="flex flex-col gap-1">
      <Track segments={segments(stability, was)} from={scale.from} to={scale.to} label={line} size="sm" minWidth={0} divided />
      <p className="text-xs text-faint">{line}</p>
    </div>
  )
}
