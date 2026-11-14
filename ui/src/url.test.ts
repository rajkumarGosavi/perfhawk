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

describe('hide Perfhawk frames flag', () => {
  it('roundtrips hph', () => {
    const s = { profile: 'ab', view: 'flame' as const, hidePh: true as const }
    expect(writeViewState(s)).toContain('hph=1')
    expect(readViewState(writeViewState(s)).hidePh).toBe(true)
    expect(readViewState('?hph=0').hidePh).toBeUndefined()
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

describe('v2 state', () => {
  it('roundtrips tx, groupBy, diff layout/filter/sort and the groups view', () => {
    const s = {
      profile: 'p', view: 'groups' as const, tx: 'rec,sub:a', groupBy: 'site' as const,
      diffLayout: 'side' as const, diffFilter: 'regressions' as const, diffSort: 'percent' as const,
    }
    const w = writeViewState(s)
    expect(w).toContain('gb=site')
    expect(w).toContain('layout=side')
    expect(w).toContain('only=regressions')
    expect(w).toContain('dsort=percent')
    expect(readViewState(w)).toEqual(s)
  })
  it('ignores unknown values', () => {
    const s = readViewState('?tx=&gb=x&layout=x&only=x&dsort=delta')
    expect(s.tx).toBeUndefined()
    expect(s.groupBy).toBeUndefined()
    expect(s.diffLayout).toBeUndefined()
    expect(s.diffFilter).toBeUndefined()
    expect(s.diffSort).toBeUndefined()
  })
})

describe('graph state', () => {
  it('roundtrips view=graph and gn', () => {
    const s = { profile: 'p', view: 'graph' as const, graphNodes: 120 }
    const w = writeViewState(s)
    expect(w).toContain('view=graph')
    expect(w).toContain('gn=120')
    expect(readViewState(w)).toEqual(s)
  })
  it('ignores out-of-range or malformed gn', () => {
    for (const bad of ['0', '4', '301', '999', 'x', '']) expect(readViewState(`?gn=${bad}`).graphNodes).toBeUndefined()
    expect(readViewState('?gn=5').graphNodes).toBe(5)
    expect(readViewState('?gn=300').graphNodes).toBe(300)
  })
})

describe('time state', () => {
  it('roundtrips window, baseline and the time view', () => {
    const s = { profile: 'p', view: 'time' as const, t0: 100, t1: 900, b0: 1000, b1: 2000 }
    const w = writeViewState(s)
    for (const part of ['view=time', 't0=100', 't1=900', 'b0=1000', 'b1=2000']) expect(w).toContain(part)
    expect(readViewState(w)).toEqual(s)
  })
  it('ignores invalid or inverted pairs', () => {
    for (const q of ['?t0=5&t1=5', '?t0=9&t1=3', '?t0=1', '?t1=9', '?t0=-1&t1=5', '?t0=x&t1=5']) {
      const s = readViewState(q)
      expect(s.t0).toBeUndefined()
      expect(s.t1).toBeUndefined()
    }
    expect(readViewState('?b0=7&b1=7').b0).toBeUndefined()
    expect(readViewState('?b0=1&b1=2').b1).toBe(2)
  })
})

describe('history state', () => {
  it('roundtrips series, kind and function with unusual characters', () => {
    const s = { view: 'history' as const, series: 'svc:é/1 x', kind: 'cpu', fn: 'main.(*T).run' }
    const w = writeViewState(s)
    expect(w).toContain('view=history')
    expect(readViewState(w)).toEqual(s)
  })
  it('omits empty values and needs no profile', () => {
    expect(writeViewState({ view: 'history' })).toBe('?view=history')
    const s = readViewState('?view=history&series=&kind=&fn=')
    expect(s.series).toBeUndefined()
    expect(s.fn).toBeUndefined()
    expect(s.profile).toBeUndefined()
  })
})
