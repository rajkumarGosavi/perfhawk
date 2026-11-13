import type { Range } from './range'

/** SVG path of a sparkline in a w x h box; empty or all-zero input gives a flat baseline. */
export function toPath(values: number[], w: number, h: number): string {
  const max = values.reduce((m, v) => (v > m ? v : m), 0)
  if (values.length === 0 || !(max > 0)) return `M0 ${h} L${w} ${h}`
  const n = values.length
  const pts = values.map((v, i) => {
    const x = n === 1 ? 0 : (i / (n - 1)) * w
    const y = h - (Math.max(0, v) / max) * h
    return `${Number(x.toFixed(2))} ${Number(y.toFixed(2))}`
  })
  if (n === 1) pts.push(`${w} ${pts[0].split(' ')[1]}`)
  return `M${pts[0]}${pts.slice(1).map((p) => ` L${p}`).join('')}`
}

/** The largest value and its first index; null for no data. */
export function peak(values: number[]): { index: number; value: number } | null {
  if (values.length === 0) return null
  let best = 0
  for (let i = 1; i < values.length; i++) if (values[i] > values[best]) best = i
  return { index: best, value: values[best] }
}

/** The time window (ms) covered by slice `index` of `points` over `durationMs`; never empty. */
export function indexToRange(index: number, points: number, durationMs: number): Range {
  const i = Math.min(Math.max(0, Math.floor(index)), Math.max(0, points - 1))
  let t0 = Math.round((i / points) * durationMs)
  const t1raw = Math.round(((i + 1) / points) * durationMs)
  let t1 = t1raw
  if (t1 <= t0) {
    if (t0 + 1 <= durationMs) t1 = t0 + 1
    else t0 = Math.max(0, t1 - 1)
  }
  return { t0, t1 }
}
