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

describe('OS-level kind guides', () => {
  it('block_io offers slowest files and top by total', () => {
    const g = kindGuide('block_io')
    expect(g.title).toBe('Block I/O')
    expect(g.actions.find((a) => a.label === 'Slowest files')?.patch).toEqual({ view: 'top' })
    expect(g.actions.some((a) => a.label === 'Top by total')).toBe(true)
  })
  it('off_cpu and sched_delay have their own titles', () => {
    expect(kindGuide('off_cpu').title).toBe('Off-CPU time')
    expect(kindGuide('sched_delay').title).toBe('Scheduler delay')
  })
  it.each(['cpu', 'wall', 'memory', 'lock', 'blocking', 'threads', 'block_io', 'off_cpu', 'sched_delay', 'nope'])(
    '%s says how it was measured',
    (k) => {
      expect(kindGuide(k).measured.length).toBeGreaterThan(10)
    },
  )
})
