import type { GcEvent, GcHeapPoint, GcPausePoint } from '../api'

/** A time window in nanoseconds since the capture started: `[from, to]`, inclusive. */
export type GcRange = [number, number]

export type GcShape = 'circle' | 'square' | 'triangle' | 'diamond'

/** "gen0", "gen1", "gen2"; runtimes without generations show "cycle". */
export function generationLabel(g: number | null | undefined): string {
  return g == null ? 'cycle' : `gen${g}`
}

/** Pause durations to one decimal in ms, or two decimals in seconds from 1 s up. */
export function formatPause(ns: number): string {
  if (ns >= 1e9) return `${(ns / 1e9).toFixed(2)} s`
  return `${(ns / 1e6).toFixed(1)} ms`
}

/** Generation is shown by shape as well as label, so colour is never the only cue. */
export function generationShape(g: number | null | undefined): GcShape {
  switch (g) {
    case 0: return 'circle'
    case 1: return 'square'
    case 2: return 'triangle'
    default: return 'diamond'
  }
}

export function filterPauses(points: GcPausePoint[], range: GcRange | null): GcPausePoint[] {
  return range ? points.filter((p) => p.startNs >= range[0] && p.startNs <= range[1]) : points
}

export function rowsInRange(events: GcEvent[], range: GcRange | null): GcEvent[] {
  return range ? events.filter((e) => e.start_ns >= range[0] && e.start_ns <= range[1]) : events
}

/** A window of `fraction` of the capture on each side of `tNs`, kept inside `[0, spanNs]`. */
export function rangeAround(tNs: number, spanNs: number, fraction = 0.05): GcRange {
  const half = spanNs * fraction
  return [Math.max(0, tNs - half), Math.min(spanNs, tNs + half)]
}

const GEN_KEYS = ['gen0', 'gen1', 'gen2', 'loh', 'poh'] as const
export type GenKey = (typeof GEN_KEYS)[number]

export type HeapRow = { tNs: number; totalBytes: number; gens: Partial<Record<GenKey, number>> }

/** Heap points with only the per-generation values the runtime reported. */
export function heapSeries(points: GcHeapPoint[]): HeapRow[] {
  return points.map((p) => {
    const gens: Partial<Record<GenKey, number>> = {}
    for (const k of GEN_KEYS) {
      const v = p[k]
      if (v !== undefined) gens[k] = v
    }
    return { tNs: p.t_ns, totalBytes: p.total_bytes, gens }
  })
}

/** Cumulative heights per point for a stacked chart; only generations present somewhere are stacked. */
export function stackedHeap(points: GcHeapPoint[]): { keys: GenKey[]; rows: { tNs: number; cum: number[] }[] } {
  const keys = GEN_KEYS.filter((k) => points.some((p) => p[k] !== undefined))
  const rows = points.map((p) => {
    let sum = 0
    return { tNs: p.t_ns, cum: keys.map((k) => (sum += p[k] ?? 0)) }
  })
  return { keys: [...keys], rows }
}
