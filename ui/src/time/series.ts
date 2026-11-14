import type { Heatmap } from '../api'
import { normalizeRange, type Range } from './range'

/** Total activity per column of a heat map: the strip's sparkline. */
export function heatToSeries(heat: Heatmap): number[] {
  const out = new Array<number>(Math.max(heat.cols, 0)).fill(0)
  for (const c of heat.cells) if (c.col >= 0 && c.col < out.length) out[c.col] += c.value
  return out
}

/** Recording length in milliseconds. */
export function heatDurationMs(heat: Heatmap): number {
  return Math.round((heat.totalBuckets * heat.bucketNs) / 1e6)
}

/** Pixel position to milliseconds in a strip `width` px wide; clamped and rounded. */
export function xToMs(x: number, width: number, durationMs: number): number {
  if (!(width > 0)) return 0
  return Math.round(Math.min(durationMs, Math.max(0, (x / width) * durationMs)))
}

/** The window a drag from x0 to x1 selects, or null when it is empty. */
export function rangeFromDrag(x0: number, x1: number, width: number, durationMs: number): Range | null {
  return normalizeRange(xToMs(x0, width, durationMs), xToMs(x1, width, durationMs), durationMs)
}
