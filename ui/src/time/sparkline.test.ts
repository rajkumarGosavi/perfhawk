import { describe, expect, it } from 'vitest'
import { indexToRange, peak, toPath } from './sparkline'

describe('toPath', () => {
  it('draws a flat baseline for empty or all-zero input, never NaN', () => {
    for (const v of [[], [0, 0], [0]]) {
      const p = toPath(v, 100, 20)
      expect(p).not.toContain('NaN')
      expect(p.startsWith('M')).toBe(true)
      expect(p).toContain('20')
    }
  })
  it('scales values into the box', () => {
    const p = toPath([1, 3, 2], 100, 30)
    expect(p).toBe('M0 20 L50 0 L100 10')
  })
  it('handles a single point without NaN', () => {
    expect(toPath([5], 100, 10)).not.toContain('NaN')
  })
})

describe('peak', () => {
  it('null for empty, first index on ties', () => {
    expect(peak([])).toBeNull()
    expect(peak([1, 5, 5, 2])).toEqual({ index: 1, value: 5 })
    expect(peak([0, 0])).toEqual({ index: 0, value: 0 })
  })
})

describe('indexToRange', () => {
  it('maps a slice to milliseconds within the session', () => {
    expect(indexToRange(0, 10, 1000)).toEqual({ t0: 0, t1: 100 })
    expect(indexToRange(9, 10, 1000)).toEqual({ t0: 900, t1: 1000 })
  })
  it('stays within bounds and never empty', () => {
    const r = indexToRange(3, 100, 50)
    expect(r.t1).toBeGreaterThan(r.t0)
    expect(r.t1).toBeLessThanOrEqual(50)
    const last = indexToRange(99, 100, 50)
    expect(last.t0).toBeGreaterThanOrEqual(0)
    expect(last.t1).toBeLessThanOrEqual(50)
    expect(last.t1).toBeGreaterThan(last.t0)
    expect(indexToRange(500, 10, 1000)).toEqual({ t0: 900, t1: 1000 })
  })
})
