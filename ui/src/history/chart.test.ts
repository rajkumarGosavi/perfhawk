import { describe, expect, it } from 'vitest'
import type { HistoryPoint } from '../api'
import { deltaFromPrevious, linePath, nearest, scale } from './chart'

const pt = (value: number, found = true, pct = value / 10): HistoryPoint => ({
  sessionId: 's', shortId: 's', profileId: 'p', startedAtUnixMs: 0, found, value, total: 100, pct, skipped: null,
})

describe('scale', () => {
  it('spaces points equally and inverts y over 0..max', () => {
    const r = scale([pt(0), pt(5), pt(10)], 100, 50, 'value')
    expect(r.map((p) => p.x)).toEqual([0, 50, 100])
    expect(r.map((p) => p.y)).toEqual([50, 25, 0])
    expect(r.every((p) => p.found)).toBe(true)
    expect(r.map((p) => p.index)).toEqual([0, 1, 2])
  })
  it('keeps not-found points as gaps', () => {
    const r = scale([pt(4), pt(0, false), pt(8)], 100, 40, 'value')
    expect(r[1].found).toBe(false)
    expect(r[1].x).toBe(50)
    expect(r[2].y).toBe(0)
  })
  it('handles a single point and all-zero values without NaN', () => {
    for (const pts of [[pt(5)], [pt(0), pt(0)], []]) {
      const r = scale(pts, 100, 40, 'value')
      for (const p of r) {
        expect(Number.isFinite(p.x)).toBe(true)
        expect(Number.isFinite(p.y)).toBe(true)
      }
    }
    expect(scale([pt(5)], 100, 40, 'value')[0].x).toBe(50)
  })
  it('uses pct when asked', () => {
    const r = scale([pt(0, true, 10), pt(0, true, 20)], 100, 10, 'pct')
    expect(r[1].y).toBe(0)
    expect(r[0].y).toBe(5)
  })
})

describe('linePath', () => {
  it('breaks the line at not-found points', () => {
    const p = linePath(scale([pt(1), pt(2), pt(0, false), pt(3), pt(4)], 100, 10, 'value'))
    expect(p.match(/M/g)).toHaveLength(2)
    expect(p).not.toContain('NaN')
  })
  it('is empty when nothing was found', () => {
    expect(linePath(scale([pt(0, false)], 100, 10, 'value'))).toBe('')
    expect(linePath([])).toBe('')
  })
  it('draws a lone point as a zero-length segment', () => {
    expect(linePath(scale([pt(2), pt(0, false), pt(3)], 100, 10, 'value')).match(/M/g)).toHaveLength(2)
  })
})

describe('deltaFromPrevious', () => {
  it('is a signed change for found neighbours', () => {
    expect(deltaFromPrevious([pt(10), pt(15)], 1, 'value')).toBe(5)
    expect(deltaFromPrevious([pt(0, true, 10), pt(0, true, 25)], 1, 'pct')).toBe(15)
  })
  it('is null for the first point, gaps and a zero previous value', () => {
    expect(deltaFromPrevious([pt(10)], 0, 'value')).toBeNull()
    expect(deltaFromPrevious([pt(10), pt(0, false)], 1, 'value')).toBeNull()
    expect(deltaFromPrevious([pt(0, false), pt(5)], 1, 'value')).toBeNull()
    expect(deltaFromPrevious([pt(0), pt(5)], 1, 'value')).toBeNull()
  })
})

describe('nearest', () => {
  it('picks the closest found point, first on ties, null when none', () => {
    const pts = scale([pt(1), pt(0, false), pt(3)], 100, 10, 'value')
    expect(nearest(pts, 10)).toBe(0)
    expect(nearest(pts, 90)).toBe(2)
    expect(nearest(pts, 50)).toBe(0)
    expect(nearest([], 5)).toBeNull()
    expect(nearest(scale([pt(0, false)], 100, 10, 'value'), 5)).toBeNull()
  })
})
