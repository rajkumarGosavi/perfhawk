import { describe, expect, it } from 'vitest'
import type { Heatmap } from '../api'
import { heatDurationMs, heatToSeries, rangeFromDrag, xToMs } from './series'

const heat = (over: Partial<Heatmap> = {}): Heatmap => ({
  bucketNs: 1e7, startUnixNs: 0, cols: 3, rows: 2, colBuckets: 1, rowBuckets: 1, cells: [], max: 0, totalBuckets: 100, ...over,
})

describe('heatToSeries', () => {
  it('sums the cells of each column', () => {
    const cells = [{ col: 0, row: 0, value: 2 }, { col: 0, row: 1, value: 3 }, { col: 2, row: 0, value: 1 }]
    expect(heatToSeries(heat({ cells }))).toEqual([5, 0, 1])
  })
  it('gives zeros when there are no cells', () => {
    expect(heatToSeries(heat())).toEqual([0, 0, 0])
  })
  it('ignores cells outside the columns', () => {
    expect(heatToSeries(heat({ cells: [{ col: 9, row: 0, value: 4 }] }))).toEqual([0, 0, 0])
  })
})

describe('xToMs', () => {
  it('maps a pixel to milliseconds, clamped', () => {
    expect(xToMs(50, 100, 1000)).toBe(500)
    expect(xToMs(-5, 100, 1000)).toBe(0)
    expect(xToMs(500, 100, 1000)).toBe(1000)
  })
  it('is 0 for an empty strip', () => {
    expect(xToMs(10, 0, 1000)).toBe(0)
  })
})

describe('rangeFromDrag', () => {
  it('orders a reversed drag', () => {
    expect(rangeFromDrag(80, 20, 100, 1000)).toEqual({ t0: 200, t1: 800 })
  })
  it('is null for a zero-width click', () => {
    expect(rangeFromDrag(40, 40, 100, 1000)).toBeNull()
  })
  it('clamps a drag released outside the strip', () => {
    expect(rangeFromDrag(50, 400, 100, 1000)).toEqual({ t0: 500, t1: 1000 })
  })
})

describe('heatDurationMs', () => {
  it('is total buckets times bucket size in ms', () => {
    expect(heatDurationMs(heat())).toBe(1000)
  })
})
