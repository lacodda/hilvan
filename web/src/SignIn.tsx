import { useState } from 'react'

import { api } from '@/api'
import { Shell } from '@/App'

/** The door. One field, because there is one learner. */
export function SignIn({ onSignedIn }: { onSignedIn: () => void }) {
  const [password, setPassword] = useState('')
  const [failed, setFailed] = useState(false)
  const [busy, setBusy] = useState(false)

  return (
    <Shell>
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          setBusy(true)
          setFailed(false)
          void api
            .logIn(password)
            .then(onSignedIn)
            .catch(() => setFailed(true))
            .finally(() => setBusy(false))
        }}
      >
        <label className="flex flex-col gap-2 text-[0.9375rem] text-dim" htmlFor="password">
          Password
          <input
            id="password"
            type="password"
            autoFocus
            autoComplete="current-password"
            className="rounded-md border border-line bg-raise px-3 py-3 text-[1.0625rem] text-text outline-none focus-visible:border-accent"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>
        {failed && (
          <p className="text-[0.9375rem] text-bad" role="alert">
            That is not the password.
          </p>
        )}
        <button
          type="submit"
          disabled={busy || password.length === 0}
          className="rounded-md bg-accent px-4 py-3 text-[0.9375rem] font-medium text-on-accent disabled:opacity-50"
        >
          {busy ? 'Checking…' : 'Come in'}
        </button>
      </form>
    </Shell>
  )
}
