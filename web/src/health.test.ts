import { describe as suite, expect, it } from 'vitest'

import { describe } from '@/health'

suite('describe', () => {
  it('says the server is up with its version', () => {
    expect(describe({ kind: 'reached', health: { status: 'ok', version: '0.1.0' } })).toBe('Server 0.1.0 is up.')
  })

  it('distinguishes a server that answers from one that can work', () => {
    const line = describe({ kind: 'reached', health: { status: 'degraded', version: '0.1.0' } })
    expect(line).toContain('cannot reach its database')
  })

  it('treats an unreachable server as a state to read through, not an error', () => {
    expect(describe({ kind: 'unreachable' })).toContain('still reads')
  })
})
