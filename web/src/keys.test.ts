import { describe, expect, it } from 'vitest'

import { keyOf } from '@/keys'

const press = (code: string, extra: Partial<{ altKey: boolean; ctrlKey: boolean; metaKey: boolean; target: unknown }> = {}) =>
  keyOf({ code, altKey: false, ctrlKey: false, metaKey: false, target: null, ...extra } as Parameters<typeof keyOf>[0])

describe('the drill from a keyboard', () => {
  it('reads the keys by their place, whatever the layout types', () => {
    // On a Russian layout the digits still type digits, but a learner
    // switching layouts mid-sitting must never find the keys dead.
    expect(press('Digit1')).toBe('1')
    expect(press('Numpad4')).toBe('4')
    expect(press('Space')).toBe('space')
    expect(press('KeyA')).toBeNull()
  })

  it('leaves a key typed into a field to the field, and one with a modifier to the browser', () => {
    expect(press('Space', { target: { tagName: 'INPUT' } })).toBeNull()
    expect(press('Digit2', { target: { tagName: 'TEXTAREA' } })).toBeNull()
    expect(press('Digit3', { target: { tagName: 'DIV', isContentEditable: true } })).toBeNull()
    expect(press('Digit1', { ctrlKey: true })).toBeNull()
    expect(press('Digit1', { target: { tagName: 'BUTTON' } })).toBe('1')
  })
})
