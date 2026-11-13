import { describe, expect, it } from 'vitest'
import { formatRange, nudge, normalizeRange } from './range'

describe('normalizeRange', () => {
  it('orders swapped ends and keeps in-range values', () => {
    expect(normalizeRange(3000, 1000, 10_000)).toEqual({ t0: 1000, t1: 3000 })
  })
  it('clamps to [0, duration]', () => {
    expect(normalizeRange(-500, 20_000, 10_000)).toEqual({ t0: 0, t1: 10_000 })
  })
  it('returns null for zero-width and beyond-duration selections', () => {
    expect(normalizeRange(500, 500, 10_000)).toBeNull()
    expect(normalizeRange(11_000, 12_000, 10_000)).toBeNull()
    expect(normalizeRange(-5, -1, 10_000)).toBeNull()
    expect(normalizeRange(0, 100, 0)).toBeNull()
  })
  it('rounds to whole milliseconds', () => {
    expect(normalizeRange(10.4, 20.6, 100)).toEqual({ t0: 10, t1: 21 })
  })
})

describe('nudge', () => {
  const r = { t0: 2000, t1: 3000 }
  it('moves left and right by the step', () => {
    expect(nudge(r, 'left', 100, 10_000)).toEqual({ t0: 1900, t1: 2900 })
    expect(nudge(r, 'right', 100, 10_000)).toEqual({ t0: 2100, t1: 3100 })
  })
  it('never leaves [0, duration] and keeps the width', () => {
    expect(nudge({ t0: 50, t1: 150 }, 'left', 100, 10_000)).toEqual({ t0: 0, t1: 100 })
    expect(nudge({ t0: 9900, t1: 9990 }, 'right', 100, 10_000)).toEqual({ t0: 9910, t1: 10_000 })
  })
  it('grows and shrinks around the centre within bounds', () => {
    expect(nudge(r, 'grow', 100, 10_000)).toEqual({ t0: 1900, t1: 3100 })
    expect(nudge(r, 'shrink', 100, 10_000)).toEqual({ t0: 2100, t1: 2900 })
    expect(nudge({ t0: 0, t1: 50 }, 'grow', 100, 10_000)).toEqual({ t0: 0, t1: 150 })
  })
  it('shrink never makes the range empty', () => {
    const s = nudge({ t0: 100, t1: 150 }, 'shrink', 100, 10_000)
    expect(s.t1).toBeGreaterThan(s.t0)
  })
})

describe('formatRange', () => {
  it('formats seconds with two decimals', () => {
    expect(formatRange({ t0: 2400, t1: 3100 })).toBe('2.40 s – 3.10 s')
    expect(formatRange({ t0: 0, t1: 50 })).toBe('0.00 s – 0.05 s')
  })
})
