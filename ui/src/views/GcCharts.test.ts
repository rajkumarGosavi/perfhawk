import { describe, expect, it } from 'vitest'
import { niceTicks, smooth } from './GcCharts'

describe('niceTicks', () => {
  it('starts at 0 and covers the max with round steps', () => {
    expect(niceTicks(248)).toEqual([0, 100, 200, 300])
    const t = niceTicks(286)
    expect(t[0]).toBe(0)
    expect(t[t.length - 1]).toBeGreaterThanOrEqual(286)
  })
  it('handles a zero max', () => {
    expect(niceTicks(0)).toEqual([0])
  })
})

describe('smooth', () => {
  it('averages neighbours and keeps the length', () => {
    expect(smooth([0, 10, 0, 10, 0], 3)).toHaveLength(5)
    expect(smooth([0, 10, 0], 3)[1]).toBeCloseTo(10 / 3)
  })
  it('is the identity for a window of 1', () => {
    expect(smooth([1, 2, 3], 1)).toEqual([1, 2, 3])
  })
})
