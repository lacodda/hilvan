import { player, usePlaying } from '@/speech'

/**
 * The button that says a sentence.
 *
 * Draws nothing when there is no sentence to say - a scaffold, a language
 * with no voice - rather than a button that does nothing. A sound that could
 * not be played shows a struck-out speaker, so a silent tap is never a
 * mystery.
 */
export function Speak({ url, label, className = '' }: { url: string | null; label: string; className?: string }) {
  const playing = usePlaying()
  if (url === null) return null
  const now = playing === url
  const failed = !now && player.hasFailed(url)

  return (
    <button
      type="button"
      onClick={() => (now ? player.stop() : void player.play(url))}
      aria-label={now ? 'Stop' : label}
      title={failed ? 'No voice answered for this sentence' : label}
      aria-pressed={now}
      className={`inline-flex size-9 shrink-0 items-center justify-center rounded-full ${
        now ? 'bg-accent text-on-accent' : failed ? 'bg-raise text-faint' : 'bg-raise text-dim'
      } ${className}`}
    >
      <SpeakerIcon state={now ? 'playing' : failed ? 'failed' : 'idle'} />
    </button>
  )
}

function SpeakerIcon({ state }: { state: 'idle' | 'playing' | 'failed' }) {
  return (
    <svg viewBox="0 0 24 24" className="size-1/2 max-w-7 min-w-[18px]" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M11 5 6 9H3v6h3l5 4z" fill="currentColor" stroke="none" />
      {state === 'failed' ? (
        <path d="m16 9 5 6m0-6-5 6" />
      ) : (
        <>
          <path d="M15.5 8.5a5 5 0 0 1 0 7" />
          {state === 'playing' && <path d="M18.5 5.5a9 9 0 0 1 0 13" />}
        </>
      )}
    </svg>
  )
}
