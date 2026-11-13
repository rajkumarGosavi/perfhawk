import { describe, expect, it } from 'vitest'
import type { TopRow } from '../api'
import { concentration, headline, kindGuide } from './guide'

const row = (selfPct: number, totalPct = selfPct): TopRow => ({
  funcId: 1,
  name: 'parse',
  file: '',
  selfValue: 42,
  total: 42,
  selfPct,
  totalPct,
})

describe('kindGuide', () => {
  it.each(['cpu', 'wall', 'memory', 'lock', 'blocking', 'threads'])('%s has questions and a flame action', (k) => {
    const g = kindGuide(k)
    expect(g.questions.length).toBeGreaterThan(0)
    expect(g.actions.some((a) => a.label === 'Flame graph')).toBe(true)
  })
  it('lock and blocking offer waits by site; threads offer identical stacks; cpu offers hottest path', () => {
    for (const k of ['lock', 'blocking']) {
      expect(kindGuide(k).actions.find((a) => a.label === 'Waits by site')?.patch).toEqual({
        view: 'groups',
        groupBy: 'site',
      })
      expect(kindGuide(k).actions.find((a) => a.label === 'Top by total')?.patch).toEqual({
        view: 'top',
        sort: 'total',
      })
    }
    expect(kindGuide('threads').actions[0]).toEqual({
      label: 'Identical stacks',
      patch: { view: 'groups', groupBy: 'stack' },
    })
    expect(kindGuide('cpu').actions.find((a) => a.label === 'Hottest path')?.patch).toEqual({
      view: 'flame',
      hot: true,
    })
  })
  it('cpu, wall, lock, blocking and the generic guide offer the call graph', () => {
    for (const k of ['cpu', 'wall', 'lock', 'blocking', 'weird']) {
      expect(kindGuide(k).actions.find((a) => a.label === 'Call graph')?.patch).toEqual({ view: 'graph' })
    }
  })
  it('unknown kinds get a generic guide', () => {
    for (const k of ['weird', '']) {
      const g = kindGuide(k)
      expect(g.questions.length).toBeGreaterThan(0)
      expect(g.actions.map((a) => a.label)).toEqual(expect.arrayContaining(['Flame graph', 'Call tree']))
    }
  })
})

describe('headline', () => {
  it('handles empty and zero rows', () => {
    expect(headline([], 'count')).toBe('No samples in this view.')
    expect(headline([row(0)], 'count')).toBe('No samples in this view.')
    expect(headline([row(NaN)], 'count')).toBe('No samples in this view.')
  })
  it('includes name and percentage', () => {
    const h = headline([row(42.1)], 'count')
    expect(h).toContain('parse')
    expect(h).toContain('42.1%')
    expect(h).not.toContain('NaN')
  })
})

describe('concentration', () => {
  it('boundary at 30', () => {
    expect(concentration([row(29.9)])).toBe('spread')
    expect(concentration([row(30)])).toBe('concentrated')
    expect(concentration([])).toBe('spread')
  })
})
