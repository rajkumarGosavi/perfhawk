import type { HistoryPoint } from '../api'

export type Metric = 'value' | 'pct'
export type ScaledPoint = { x: number; y: number; found: boolean; index: number }

const metricOf = (p: HistoryPoint, m: Metric) => (m === 'pct' ? p.pct : p.value)

/** Equal horizontal spacing by session order; y inverted over 0..max of the found points. */
export function scale(points: HistoryPoint[], w: number, h: number, metric: Metric): ScaledPoint[] {
  const n = points.length
  const max = points.reduce((m, p) => (p.found && metricOf(p, metric) > m ? metricOf(p, metric) : m), 0)
  return points.map((p, index) => {
    const x = n <= 1 ? w / 2 : (index / (n - 1)) * w
    const y = p.found && max > 0 ? h - (Math.max(0, metricOf(p, metric)) / max) * h : h
    return { x, y, found: p.found, index }
  })
}

/** A path through the found points; the line breaks wherever a point is missing. */
export function linePath(pts: ScaledPoint[]): string {
  const parts: string[] = []
  let open = false
  let last: ScaledPoint | null = null
  const fmt = (n: number) => Number(n.toFixed(2))
  for (const p of pts) {
    if (!p.found) {
      if (open && last) parts.push(`L${fmt(last.x)} ${fmt(last.y)}`)
      open = false
      last = null
      continue
    }
    if (!open) {
      parts.push(`M${fmt(p.x)} ${fmt(p.y)}`)
      open = true
      last = null
    } else {
      parts.push(`L${fmt(p.x)} ${fmt(p.y)}`)
    }
    last = p
  }
  // A lone point becomes a zero-length segment so it still draws.
  const joined = parts.join(' ')
  return joined.replace(/M(\S+ \S+)(?= M|$)/g, 'M$1 L$1')
}

/** Change from the previous session; null for the first point, gaps or a zero baseline. */
export function deltaFromPrevious(points: HistoryPoint[], i: number, metric: Metric): number | null {
  if (i <= 0 || i >= points.length) return null
  const prev = points[i - 1]
  const cur = points[i]
  if (!prev.found || !cur.found) return null
  const base = metricOf(prev, metric)
  if (!(base > 0)) return null
  return metricOf(cur, metric) - base
}

/** Index of the found point closest to x; the first wins ties. */
export function nearest(pts: ScaledPoint[], x: number): number | null {
  let best: number | null = null
  let dist = Infinity
  for (const p of pts) {
    if (!p.found) continue
    const d = Math.abs(p.x - x)
    if (d < dist) {
      dist = d
      best = p.index
    }
  }
  return best
}
