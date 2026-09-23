import { use, useState } from 'react'

import { api, speechUrl, type LanguageVoices, type Voice, type VoicesScreen as Screen } from '@/api'
import { Shell } from '@/App'
import { engineLine, forecastLine, languageName, remainingLine } from '@/budget'
import { Speak } from '@/Speak'
import { player, useStopOnLeave } from '@/speech'

/**
 * Who speaks each language, and what is left of the budget.
 *
 * A voice is chosen by hearing it: picking one says the language's sample
 * sentence in it straight away.
 */
export function VoicesScreen({ screen, onChanged, onLeave }: { screen: Promise<Screen>; onChanged: () => void; onLeave: () => void }) {
  const voices = use(screen)
  useStopOnLeave()

  return (
    <Shell>
      <h2 className="text-2xl font-semibold tracking-tight">Voices</h2>

      {voices.languages.length === 0 && <p className="text-[1.0625rem] text-dim">Load a pack first - the voices follow its languages.</p>}

      {voices.languages.map((language) => (
        <Language key={language.code} language={language} onChanged={onChanged} />
      ))}

      <section className="flex flex-col gap-2 rounded-md bg-raise p-4">
        <h3 className="text-[0.8125rem] font-medium tracking-caption text-dim uppercase">ElevenLabs budget</h3>
        {voices.budget ? (
          <>
            <p className="text-[1.0625rem]">{remainingLine(voices.budget)}</p>
            <p className={`text-[0.9375rem] ${voices.budget.lasts ? 'text-dim' : 'text-bad'}`}>{forecastLine(voices.budget)}</p>
          </>
        ) : (
          <p className="text-[0.9375rem] text-dim">
            {voices.elevenlabs === 'off'
              ? 'No ElevenLabs key: the language you learn is spoken by Piper. Add HILVAN_ELEVENLABS_KEY to the stand to give it a native voice.'
              : 'The account did not answer; the budget shows when it does.'}
          </p>
        )}
        <p className="text-[0.8125rem] text-faint">
          {engineLine('Piper', voices.piper)} {engineLine('ElevenLabs', voices.elevenlabs)}
        </p>
      </section>

      <footer className="mt-auto pt-6">
        <button type="button" onClick={onLeave} className="text-[0.8125rem] text-faint underline">
          Back to today
        </button>
      </footer>
    </Shell>
  )
}

function Language({ language, onChanged }: { language: LanguageVoices; onChanged: () => void }) {
  const [saving, setSaving] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const current = language.spoken?.voice
  const sample = language.sample ? speechUrl(language.code, language.sample, 'normal') : null

  const choose = (voice: Voice) => {
    setSaving(voice.id)
    setFailed(false)
    void api
      .chooseVoice(language.code, voice)
      .then(() => {
        // The sample sentence in the voice just chosen: the server now
        // answers with it, and a new ETag keeps the browser from the old one.
        if (sample) void player.play(sample)
        onChanged()
      })
      .catch(() => setFailed(true))
      .finally(() => setSaving(null))
  }

  return (
    <section className="flex flex-col gap-3">
      <header className="flex items-center justify-between gap-3">
        <h3 className="text-[1.0625rem] font-medium">
          {languageName(language.code)}{' '}
          <span className="text-[0.8125rem] font-normal text-faint">{language.role === 'native' ? 'your language' : 'learning'}</span>
        </h3>
        <Speak url={sample} label={`Hear ${current?.name ?? 'the voice'}`} />
      </header>
      {language.options.length === 0 ? (
        <p className="text-[0.9375rem] text-dim">No voice speaks it right now.</p>
      ) : (
        <ul className="flex flex-col gap-1" role="radiogroup" aria-label={`Voice for ${languageName(language.code)}`}>
          {language.options.map((voice) => {
            const chosen = current?.engine === voice.engine && current.id === voice.id
            return (
              <li key={`${voice.engine}:${voice.id}`}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={chosen}
                  disabled={saving !== null}
                  onClick={() => !chosen && choose(voice)}
                  className={`flex w-full items-center justify-between rounded-md px-3 py-3 text-left ${chosen ? 'bg-accent text-on-accent' : 'bg-raise'}`}
                >
                  <span className="text-[0.9375rem] font-medium">{voice.name}</span>
                  <span className={`text-[0.75rem] ${chosen ? 'text-on-accent/80' : 'text-faint'}`}>
                    {saving === voice.id ? 'saving…' : voice.engine === 'elevenlabs' ? 'ElevenLabs' : 'Piper'}
                  </span>
                </button>
              </li>
            )
          })}
        </ul>
      )}
      {language.spoken && !language.spoken.chosen && <p className="text-[0.8125rem] text-faint">Not chosen yet - {language.spoken.voice.name} speaks it by default.</p>}
      {failed && <p className="text-[0.8125rem] text-bad">That voice could not be chosen.</p>}
    </section>
  )
}
