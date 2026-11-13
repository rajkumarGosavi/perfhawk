import { describe, expect, it } from 'vitest'
import { mergeViewState, readViewState, writeViewState, type ViewState } from './url'

describe('url state', () => {
  it('roundtrips', () => {
    const s = { profile: 'ab12', view: 'top' as const, st: 1, focus: 'json' }
    const w = writeViewState(s)
    expect(readViewState(w)).toEqual(s)
  })
  it('default state writes empty string', () => {
    expect(writeViewState({ view: 'summary' })).toBe('')
    expect(readViewState('')).toEqual({ view: 'summary' })
  })
  it('roundtrips sandwich with func and search, ignores bad view', () => {
    const s = { view: 'sandwich' as const, func: 3, search: 'a b', exclude: 'x', hide: 'y' }
    expect(readViewState(writeViewState(s))).toEqual(s)
    expect(readViewState('?view=nope').view).toBe('summary')
  })
})

describe('summary default and hot flag', () => {
  it('defaults missing or unknown view to summary', () => {
    expect(readViewState('?profile=ab').view).toBe('summary')
    expect(readViewState('?profile=ab&view=bogus').view).toBe('summary')
  })
  it('writes flame explicitly and omits summary', () => {
    expect(writeViewState({ profile: 'ab', view: 'flame' })).toBe('?profile=ab&view=flame')
    expect(writeViewState({ profile: 'ab', view: 'summary' })).toBe('?profile=ab')
  })
  it('roundtrips new views', () => {
    for (const view of ['tree', 'both', 'summary', 'flame', 'icicle'] as const) {
      const s = { profile: 'ab', view }
      expect(readViewState(writeViewState(s))).toEqual(s)
    }
  })
  it('roundtrips hot', () => {
    const s = { profile: 'ab', view: 'tree' as const, hot: true as const }
    expect(writeViewState(s)).toContain('hot=1')
    expect(readViewState(writeViewState(s)).hot).toBe(true)
    expect(readViewState('?hot=0').hot).toBeUndefined()
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
