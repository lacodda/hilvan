/** What `GET /api/health` answers. */
export interface Health {
  status: 'ok' | 'degraded'
  version: string
}

/** The one line the shell shows about the server it is talking to. */
export type ServerState = { kind: 'checking' } | { kind: 'unreachable' } | { kind: 'reached'; health: Health }

/** Asks the server how it is, and never throws: an unreachable server is a state, not an error. */
export async function fetchHealth(): Promise<ServerState> {
  try {
    const response = await fetch('/api/health')
    if (!response.ok && response.status !== 503) {
      return { kind: 'unreachable' }
    }
    const health = (await response.json()) as Health
    return { kind: 'reached', health }
  } catch {
    return { kind: 'unreachable' }
  }
}

/** The state as a sentence for the shell. */
export function describe(state: ServerState): string {
  switch (state.kind) {
    case 'checking':
      return 'Reaching the server…'
    case 'unreachable':
      return 'The server is not reachable. What is cached still reads.'
    case 'reached':
      return state.health.status === 'ok'
        ? `Server ${state.health.version} is up.`
        : `Server ${state.health.version} is up but cannot reach its database.`
  }
}
