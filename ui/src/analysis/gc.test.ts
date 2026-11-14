import { describe, expect, it } from 'vitest'
import type { GcEvent, GcHeapPoint, GcPausePoint } from '../api'
import {
  filterPauses, formatPause, generationLabel, generationShape, heapSeries, rangeAround, rowsInRange, stackedHeap,
} from './gc'

const pause = (id: number, startNs: number, g: number | null = 0): GcPausePoint => ({ id, startNs, pauseNs: 1, generation: g })
const event = (id: number, start_ns: number): GcEvent => ({
  id, start_ns, kind: 'blocking', reason: 'allocation', induced: false, pause_ns: 1, duration_ns: 2,
})

describe('generationLabel', () => {
  it('names generations and generation-less cycles', () => {
    expect(generationLabel(0)).toBe('gen0')
    expect(generationLabel(1)).toBe('gen1')
    expect(generationLabel(2)).toBe('gen2')
    expect(generationLabel(null)).toBe('cycle')
    expect(generationLabel(undefined)).toBe('cycle')
  })
})

describe('formatPause', () => {
  it('formats at the boundaries', () => {
    expect(formatPause(0)).toBe('0.0 ms')
    expect(formatPause(400_000)).toBe('0.4 ms')
    expect(formatPause(12_300_000)).toBe('12.3 ms')
    expect(formatPause(1_200_000_000)).toBe('1.20 s')
  })
})

describe('range filters', () => {
  it('filterPauses is inclusive and null means everything', () => {
    const ps = [pause(1, 10), pause(2, 20), pause(3, 30)]
    expect(filterPauses(ps, [10, 20]).map((p) => p.id)).toEqual([1, 2])
    expect(filterPauses(ps, null)).toHaveLength(3)
    expect(filterPauses([], [0, 1])).toEqual([])
  })
  it('rowsInRange filters events by start time', () => {
    const es = [event(1, 5), event(2, 15), event(3, 25)]
    expect(rowsInRange(es, [10, 25]).map((e) => e.id)).toEqual([2, 3])
    expect(rowsInRange(es, null)).toHaveLength(3)
  })
  it('rangeAround clamps to the capture', () => {
    expect(rangeAround(500, 1000)).toEqual([450, 550])
    expect(rangeAround(10, 1000)).toEqual([0, 60])
    expect(rangeAround(990, 1000)).toEqual([940, 1000])
  })
})

describe('heap helpers', () => {
  const pts: GcHeapPoint[] = [
    { t_ns: 1, total_bytes: 400, gen0: 100, gen2: 300 },
    { t_ns: 2, total_bytes: 500 },
  ]
  it('heapSeries omits absent generations instead of emitting undefined', () => {
    const s = heapSeries(pts)
    expect(s[0]).toEqual({ tNs: 1, totalBytes: 400, gens: { gen0: 100, gen2: 300 } })
    expect(s[1]).toEqual({ tNs: 2, totalBytes: 500, gens: {} })
    expect(Object.keys(s[1].gens)).toHaveLength(0)
  })
  it('stackedHeap stacks only the generations present, absent counts as 0', () => {
    const st = stackedHeap(pts)
    expect(st.keys).toEqual(['gen0', 'gen2'])
    expect(st.rows[0].cum).toEqual([100, 400])
    expect(st.rows[1].cum).toEqual([0, 0])
  })
  it('empty input gives empty output', () => {
    expect(heapSeries([])).toEqual([])
    expect(stackedHeap([])).toEqual({ keys: [], rows: [] })
  })
})

describe('generationShape', () => {
  it('is distinct per generation so colour is never the only cue', () => {
    const shapes = [0, 1, 2, null].map((g) => generationShape(g))
    expect(new Set(shapes).size).toBe(4)
  })
})
