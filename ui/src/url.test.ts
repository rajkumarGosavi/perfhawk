import { describe, expect, it } from 'vitest'
import { mergeViewState, readViewState, writeViewState, type ViewState } from './url'

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

describe('sort state', () => {
  it('roundtrips total and omits the self default', () => {
    const s = { profile: 'ab12', view: 'top' as const, sort: 'total' as const }
    expect(writeViewState(s)).toContain('sort=total')
    expect(readViewState(writeViewState(s))).toEqual(s)
    expect(writeViewState({ view: 'top', sort: 'self' })).not.toContain('sort')
    expect(readViewState('?view=top&sort=bogus').sort).toBeUndefined()
  })
})

describe('mergeViewState', () => {
  it('two patches built from the same stale base both survive when applied sequentially', () => {
    const base: ViewState = { profile: 'ab12', view: 'flame' }
    let cur: ViewState = base
    cur = mergeViewState(cur, { focus: 'json' }) // patch from stale base
    cur = mergeViewState(cur, { exclude: 'gc' }) // second patch, same stale base
    expect(cur).toEqual({ profile: 'ab12', view: 'flame', focus: 'json', exclude: 'gc' })
    expect(mergeViewState(cur, { view: 'top' }).focus).toBe('json')
  })
  it('removes keys patched to undefined', () => {
    expect(mergeViewState({ view: 'flame', focus: 'x' }, { focus: undefined })).toEqual({ view: 'flame' })
  })
})

describe('diff state', () => {
  it('roundtrips profile, diff view, base and normalize', () => {
    const s = { profile: 'head1', view: 'diff' as const, base: 'base1', normalize: 'total' as const }
    const w = writeViewState(s)
    expect(w).toContain('view=diff')
    expect(w).toContain('base=base1')
    expect(w).toContain('normalize=total')
    expect(readViewState(w)).toEqual(s)
  })
  it('omits normalize when none and ignores bad values', () => {
    expect(writeViewState({ profile: 'h', view: 'diff', base: 'b' })).not.toContain('normalize')
    expect(readViewState('?view=diff&normalize=bogus').normalize).toBeUndefined()
  })
  it('roundtrips session and depth', () => {
    const s = { session: 'abc123', view: 'flame' as const, depth: 3 }
    expect(readViewState(writeViewState(s))).toEqual(s)
    expect(readViewState('?depth=0').depth).toBeUndefined()
  })
})
