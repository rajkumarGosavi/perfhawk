import { describe, expect, it } from 'vitest'
import { readViewState, writeViewState } from './url'

describe('url state', () => {
  it('roundtrips', () => {
    const s = { profile: 'ab12', view: 'top' as const, st: 1, focus: 'json' }
    const w = writeViewState(s)
    expect(readViewState(w)).toEqual(s)
  })
  it('default state writes empty string', () => {
    expect(writeViewState({ view: 'flame' })).toBe('')
    expect(readViewState('')).toEqual({ view: 'flame' })
  })
  it('roundtrips sandwich with func and search, ignores bad view', () => {
    const s = { view: 'sandwich' as const, func: 3, search: 'a b', exclude: 'x', hide: 'y' }
    expect(readViewState(writeViewState(s))).toEqual(s)
    expect(readViewState('?view=nope').view).toBe('flame')
  })
})
