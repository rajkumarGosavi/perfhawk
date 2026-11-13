import { describe, expect, it } from 'vitest'
import type { Heatmap } from '../api'
import { cellColor, cellRect, pointToTime, timeToX } from './heatLayout'

const heat = (over: Partial<Heatmap> = {}): Heatmap => ({
  bucketNs: 20_000_000, startUnixNs: 0, cols: 10, rows: 50, colBuckets: 50, rowBuckets: 1,
  cells: [], max: 4, totalBuckets: 500, ...over,
})

describe('cellRect', () => {
  it('places cells on a cols x rows grid', () => {
    const h = heat()
    const r = cellRect(h, { col: 2, row: 5, value: 1 }, 1000, 500)
    expect(r).toEqual({ x: 200, y: 50, w: 100, h: 10 })
  })
  it('handles a degenerate heatmap without NaN', () => {
    const r = cellRect(heat({ cols: 0, rows: 0 }), { col: 0, row: 0, value: 1 }, 100, 100)
    for (const v of Object.values(r)) expect(Number.isFinite(v)).toBe(true)
  })
})

describe('pointToTime / timeToX', () => {
  it.each([[1000, 500], [640, 300], [333, 120]])('roundtrips the column at %ix%i', (w, hgt) => {
    const h = heat()
    const cw = w / h.cols
    for (const x of [1, cw * 3 + 2, w - 1]) {
      const ms = pointToTime(h, x, hgt / 2, w, hgt)
      const back = timeToX(h, ms, w)
      expect(Math.floor(back / cw)).toBe(Math.floor(x / cw))
    }
  })
  it('maps the top-left to time zero and clamps outside points', () => {
    const h = heat()
    expect(pointToTime(h, 0, 0, 1000, 500)).toBe(0)
    expect(pointToTime(h, -50, -50, 1000, 500)).toBe(0)
    const last = pointToTime(h, 5000, 5000, 1000, 500)
    expect(last).toBeGreaterThan(0)
    expect(last).toBeLessThanOrEqual((h.totalBuckets * h.bucketNs) / 1e6)
  })
  it('a column is col_buckets buckets wide in time', () => {
    const h = heat()
    // column 1 starts at bucket 50 = 1000 ms at 20 ms buckets
    expect(pointToTime(h, 150, 0, 1000, 500)).toBe(1000)
  })
})

describe('cellColor', () => {
  it('is 0 for a zero max and 0..1 otherwise', () => {
    expect(cellColor(5, 0)).toBe(0)
    expect(cellColor(0, 10)).toBe(0)
    expect(cellColor(10, 10)).toBe(1)
    const mid = cellColor(5, 10)
    expect(mid).toBeGreaterThan(0)
    expect(mid).toBeLessThan(1)
    expect(cellColor(50, 10)).toBe(1)
  })
})
