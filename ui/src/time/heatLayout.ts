import type { HeatCell, Heatmap } from '../api'

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

export function cellRect(h: Heatmap, c: HeatCell, w: number, hgt: number): { x: number; y: number; w: number; h: number } {
  const cw = w / Math.max(h.cols, 1)
  const ch = hgt / Math.max(h.rows, 1)
  return { x: c.col * cw, y: c.row * ch, w: cw, h: ch }
}

/** Start time (ms) of the cell under a point, clamped to the capture. */
export function pointToTime(h: Heatmap, x: number, y: number, w: number, hgt: number): number {
  const cw = w / Math.max(h.cols, 1)
  const ch = hgt / Math.max(h.rows, 1)
  const col = clamp(Math.floor(x / cw), 0, Math.max(h.cols - 1, 0))
  const row = clamp(Math.floor(y / ch), 0, Math.max(h.rows - 1, 0))
  const bucket = col * h.colBuckets + row * h.rowBuckets
  const ms = Math.round((bucket * h.bucketNs) / 1e6)
  return clamp(ms, 0, Math.round((h.totalBuckets * h.bucketNs) / 1e6))
}

/** Horizontal position of a time; the column it falls in. */
export function timeToX(h: Heatmap, ms: number, w: number): number {
  const bucket = (ms * 1e6) / Math.max(h.bucketNs, 1)
  const col = bucket / Math.max(h.colBuckets, 1)
  return clamp((col / Math.max(h.cols, 1)) * w, 0, w)
}

/** 0..1 intensity; square-root scaled so faint activity stays visible. */
export function cellColor(value: number, max: number): number {
  if (!(max > 0) || !(value > 0)) return 0
  return Math.min(1, Math.sqrt(value / max))
}
