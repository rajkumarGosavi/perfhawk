/** A time window in milliseconds from the start of the capture. */
export type Range = { t0: number; t1: number }

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** Orders the ends, clamps to [0, durationMs] and rounds; null when nothing is left. */
export function normalizeRange(a: number, b: number, durationMs: number): Range | null {
  if (!(durationMs > 0) || !Number.isFinite(a) || !Number.isFinite(b)) return null
  const t0 = clamp(Math.round(Math.min(a, b)), 0, durationMs)
  const t1 = clamp(Math.round(Math.max(a, b)), 0, durationMs)
  return t1 > t0 ? { t0, t1 } : null
}

export function nudge(r: Range, dir: 'left' | 'right' | 'grow' | 'shrink', stepMs: number, durationMs: number): Range {
  const width = r.t1 - r.t0
  switch (dir) {
    case 'left':
    case 'right': {
      const t0 = clamp(r.t0 + (dir === 'left' ? -stepMs : stepMs), 0, Math.max(0, durationMs - width))
      return { t0, t1: t0 + width }
    }
    case 'grow':
      return { t0: Math.max(0, r.t0 - stepMs), t1: Math.min(durationMs, r.t1 + stepMs) }
    case 'shrink': {
      const t0 = r.t0 + stepMs
      const t1 = r.t1 - stepMs
      if (t1 > t0) return { t0, t1 }
      const mid = Math.floor((r.t0 + r.t1) / 2)
      return { t0: mid, t1: mid + 1 }
    }
  }
}

const sec = (ms: number) => `${(ms / 1000).toFixed(2)} s`

export function formatRange(r: Range): string {
  return `${sec(r.t0)} – ${sec(r.t1)}`
}
