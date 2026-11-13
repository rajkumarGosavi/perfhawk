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
  it('lock and blocking offer longest waits; cpu offers hottest path', () => {
    expect(kindGuide('lock').actions.find((a) => a.label === 'Longest waits (by total)')?.patch).toEqual({
      view: 'top',
      sort: 'total',
    })
    expect(kindGuide('cpu').actions.find((a) => a.label === 'Hottest path')?.patch).toEqual({
      view: 'flame',
      hot: true,
    })
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
