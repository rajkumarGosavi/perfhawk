import { useState } from 'react'
import type { GcHeapPoint, GcPausePoint, GcRatePoint } from '../api'
import { formatValue } from '../format'
import {
  formatPause, generationLabel, generationShape, rangeAround, stackedHeap, type GcRange, type GcShape,
} from '../analysis/gc'
import { EmptyState } from '../ui/EmptyState'

const W = 1200
const H = 230

// Line and area charts are taller than the pause timeline so they stay readable side by side.
const LW = 640
const LH = 280

// Plot area margins: room for value labels on the left and time labels below.
const ML = 62
const MR = 10
const MT = 10
const MB = 38

const x = (tNs: number, durationNs: number, w = W) => ML + (durationNs > 0 ? (tNs / durationNs) * (w - ML - MR) : 0)
const y = (v: number, max: number, h = H) => h - MB - (max > 0 ? (v / max) * (h - MT - MB) : 0)

function seconds(ns: number): string {
  return `${(ns / 1e9).toFixed(2)} s`
}

/** Round tick values from 0 up to just above `max`: 1, 2 or 5 times a power of ten. */
export function niceTicks(max: number, count = 4): number[] {
  if (!(max > 0)) return [0]
  const raw = max / count
  const mag = 10 ** Math.floor(Math.log10(raw))
  const step = ([1, 2, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag
  const out: number[] = []
  for (let v = 0; ; v += step) {
    out.push(v)
    if (v >= max) break
  }
  return out
}

/** Moving average over `window` samples (the series is bucketed evenly), to read a noisy rate. */
export function smooth(values: number[], window: number): number[] {
  if (window <= 1) return values
  const half = Math.floor(window / 2)
  return values.map((_, i) => {
    const from = Math.max(0, i - half)
    const to = Math.min(values.length - 1, i + half)
    let sum = 0
    for (let k = from; k <= to; k++) sum += values[k]
    return sum / (to - from + 1)
  })
}

/**
 * Gridlines, value labels on the left, time labels below, and an axis title on each side.
 * `max` is the value at the top of the plot; `fmt` formats a value tick.
 */
function Axes({
  w, h, durationNs, max, fmt, yTitle, ticks = niceTicks(max),
}: { w: number; h: number; durationNs: number; max: number; fmt: (v: number) => string; yTitle: string; ticks?: number[] }) {
  const xTicks = niceTicks(durationNs / 1e9, 5)
  return (
    <g className="gc-axes" aria-hidden="true">
      {ticks.map((v) => (
        <g key={v}>
          <line x1={ML} x2={w - MR} y1={y(v, max, h)} y2={y(v, max, h)} className="gc-grid" />
          <text x={ML - 6} y={y(v, max, h) + 4} textAnchor="end" className="gc-tick">{fmt(v)}</text>
        </g>
      ))}
      {xTicks.map((sec) => (
        <g key={sec}>
          <line x1={x(sec * 1e9, durationNs, w)} x2={x(sec * 1e9, durationNs, w)} y1={h - MB} y2={h - MB + 4} className="gc-grid" />
          <text x={x(sec * 1e9, durationNs, w)} y={h - MB + 17} textAnchor="middle" className="gc-tick">{`${sec}s`}</text>
        </g>
      ))}
      <line x1={ML} x2={w - MR} y1={h - MB} y2={h - MB} className="gc-axis" />
      <line x1={ML} x2={ML} y1={MT} y2={h - MB} className="gc-axis" />
      <text x={(ML + w - MR) / 2} y={h - 4} textAnchor="middle" className="gc-tick">Time since capture start</text>
      <text transform={`translate(11 ${(MT + h - MB) / 2}) rotate(-90)`} textAnchor="middle" className="gc-tick">{yTitle}</text>
    </g>
  )
}

function RangeBand({ range, durationNs, w = W, h = H }: { range: GcRange | null; durationNs: number; w?: number; h?: number }) {
  if (!range) return null
  const x0 = x(range[0], durationNs, w)
  return <rect x={x0} y={0} width={Math.max(2, x(range[1], durationNs, w) - x0)} height={h} className="gc-band" />
}

function Marker({ shape, cx, cy }: { shape: GcShape; cx: number; cy: number }) {
  const r = 4
  switch (shape) {
    case 'circle': return <circle cx={cx} cy={cy} r={r} />
    case 'square': return <rect x={cx - r} y={cy - r} width={2 * r} height={2 * r} />
    case 'triangle': return <polygon points={`${cx},${cy - r - 1} ${cx - r - 1},${cy + r} ${cx + r + 1},${cy + r}`} />
    default: return <polygon points={`${cx},${cy - r - 1} ${cx + r + 1},${cy} ${cx},${cy + r + 1} ${cx - r - 1},${cy}`} />
  }
}

/** Heap size over time, stacked by generation when the runtime reports generations. */
export function HeapChart({
  points, durationNs, range,
}: { points: GcHeapPoint[]; durationNs: number; range: GcRange | null }) {
  if (points.length === 0) return <p className="muted">This runtime reported no heap sizes.</p>
  const st = stackedHeap(points)
  const peak = Math.max(...points.map((p) => p.total_bytes), 1)
  const layers = st.keys.map((k, i) => {
    const top = st.rows.map((r) => `${x(r.tNs, durationNs, LW)},${y(r.cum[i], peak, LH)}`)
    const bottom = st.rows.map((r) => `${x(r.tNs, durationNs, LW)},${y(i === 0 ? 0 : r.cum[i - 1], peak, LH)}`).reverse()
    return { key: k, d: `M${[...top, ...bottom].join(' L')} Z` }
  })
  const line = points.map((p) => `${x(p.t_ns, durationNs, LW)},${y(p.total_bytes, peak, LH)}`).join(' L')
  const last = st.rows[st.rows.length - 1]
  return (
    <figure className="gc-chart">
      <figcaption>Heap size over time <span className="muted">(bytes in use by the program; largest sample {formatValue(peak, 'bytes')})</span></figcaption>
      <svg viewBox={`0 0 ${LW} ${LH}`} role="img" aria-label={`Heap size over time, largest sample ${formatValue(peak, 'bytes')}. A data table follows.`}>
        <Axes w={LW} h={LH} durationNs={durationNs} max={peak} fmt={(v) => formatValue(v, 'bytes')} yTitle="Heap size" />
        <RangeBand range={range} durationNs={durationNs} w={LW} h={LH} />
        {layers.map((l, i) => (
          <path key={l.key} d={l.d} fill="currentColor" fillOpacity={0.12 + i * 0.12} stroke="currentColor"
            strokeWidth={1} strokeDasharray={i % 2 ? '4 2' : undefined} vectorEffect="non-scaling-stroke" />
        ))}
        <path d={`M${line}`} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
        {last && st.keys.map((k, i) => (
          <text key={k} x={LW - MR - 4} y={y((last.cum[i] + (i === 0 ? 0 : last.cum[i - 1])) / 2, peak, LH)}
            textAnchor="end" className="gc-label">{k}</text>
        ))}
      </svg>
      <details>
        <summary>Data table</summary>
        <table className="gc-table">
          <thead>
            <tr><th scope="col">Time</th><th scope="col">Total</th>{st.keys.map((k) => <th key={k} scope="col">{k}</th>)}</tr>
          </thead>
          <tbody>
            {points.map((p) => (
              <tr key={p.t_ns}>
                <td>{seconds(p.t_ns)}</td>
                <td>{formatValue(p.total_bytes, 'bytes')}</td>
                {st.keys.map((k) => <td key={k}>{p[k] === undefined ? '–' : formatValue(p[k] as number, 'bytes')}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  )
}

/** Allocation rate (bytes per second) over time. */
export function AllocChart({
  points, durationNs, range,
}: { points: GcRatePoint[]; durationNs: number; range: GcRange | null }) {
  if (points.length === 0) return <EmptyState title="No allocation data was captured." />
  // Buckets are tens of milliseconds wide, so the raw line is noise; show a one-second average.
  const bucketNs = points.length > 1 ? points[1].tNs - points[0].tNs : 1e9
  const window = Math.max(1, Math.round(1e9 / Math.max(bucketNs, 1)))
  const avg = smooth(points.map((p) => p.bytesPerS), window)
  const peak = Math.max(...avg, 1)
  const ticks = niceTicks(peak)
  const top = ticks[ticks.length - 1]
  const line = points.map((p, i) => `${x(p.tNs, durationNs, LW)},${y(avg[i], top, LH)}`).join(' L')
  return (
    <figure className="gc-chart">
      <figcaption>Allocation rate <span className="muted">(bytes allocated per second, 1 s average; peak {formatValue(peak, 'bytes')}/s)</span></figcaption>
      <svg viewBox={`0 0 ${LW} ${LH}`} role="img" aria-label={`Allocation rate over time, peak ${formatValue(peak, 'bytes')} per second. A data table follows.`}>
        <Axes w={LW} h={LH} durationNs={durationNs} max={top} ticks={ticks} fmt={(v) => `${formatValue(v, 'bytes')}/s`} yTitle="Allocated per second" />
        <RangeBand range={range} durationNs={durationNs} w={LW} h={LH} />
        <path d={`M${line}`} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      </svg>
      <details>
        <summary>Data table</summary>
        <table className="gc-table">
          <thead><tr><th scope="col">Time</th><th scope="col">Rate</th></tr></thead>
          <tbody>
            {points.map((p) => (
              <tr key={p.tNs}><td>{seconds(p.tNs)}</td><td>{formatValue(p.bytesPerS, 'bytes')}/s</td></tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  )
}

/**
 * One mark per GC: height = pause, shape = generation. Drag to select a time window, or focus
 * the chart and use Left/Right to move between GCs, Enter to select the window around one and
 * Escape to clear. The GC table below is the text alternative.
 */
export function PauseTimeline({
  pauses, durationNs, range, onRange,
}: { pauses: GcPausePoint[]; durationNs: number; range: GcRange | null; onRange: (r: GcRange | null) => void }) {
  const [cursor, setCursor] = useState(0)
  const [dragFrom, setDragFrom] = useState<number | null>(null)
  const [dragTo, setDragTo] = useState<number | null>(null)
  if (pauses.length === 0) return null
  const ordered = [...pauses].sort((a, b) => a.startNs - b.startNs)
  const peak = Math.max(...ordered.map((p) => p.pauseNs), 1)
  const at = (clientX: number, el: SVGSVGElement) => {
    const r = el.getBoundingClientRect()
    return Math.min(durationNs, Math.max(0, ((clientX - r.left) / r.width) * durationNs))
  }
  const cur = ordered[Math.min(cursor, ordered.length - 1)]
  const shown: GcRange | null = dragTo !== null && dragFrom !== null
    ? [Math.min(dragFrom, dragTo), Math.max(dragFrom, dragTo)]
    : range
  return (
    <figure className="gc-chart">
      <figcaption>
        Pauses <span className="muted">(each mark is one GC: when it ran and how long the program was stopped; longest {formatPause(peak)})</span>
      </figcaption>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        role="group"
        tabIndex={0}
        aria-label={`Pause timeline, ${ordered.length} GCs. Left and Right move between GCs, Enter selects the time window around the current one, Escape clears the selection.`}
        onPointerDown={(e) => { const t = at(e.clientX, e.currentTarget); setDragFrom(t); setDragTo(t) }}
        onPointerMove={(e) => { if (dragFrom !== null) setDragTo(at(e.clientX, e.currentTarget)) }}
        onPointerUp={(e) => {
          const from = dragFrom
          const to = at(e.clientX, e.currentTarget)
          setDragFrom(null)
          setDragTo(null)
          if (from === null) return
          const r = e.currentTarget.getBoundingClientRect()
          const movedPx = (Math.abs(to - from) / durationNs) * r.width
          onRange(movedPx > 3 ? [Math.min(from, to), Math.max(from, to)] : rangeAround(to, durationNs))
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft') { e.preventDefault(); setCursor((c) => Math.max(0, c - 1)) }
          else if (e.key === 'ArrowRight') { e.preventDefault(); setCursor((c) => Math.min(ordered.length - 1, c + 1)) }
          else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onRange(rangeAround(cur.startNs, durationNs)) }
          else if (e.key === 'Escape') { onRange(null) }
        }}
      >
        <Axes w={W} h={H} durationNs={durationNs} max={peak} fmt={formatPause} yTitle="Pause length" />
        <RangeBand range={shown} durationNs={durationNs} />
        <g fill="currentColor" stroke="currentColor" strokeWidth={1}>
          {ordered.map((p) => {
            const cx = x(p.startNs, durationNs)
            const cy = y(p.pauseNs, peak)
            return (
              <g key={p.id}>
                <line x1={cx} x2={cx} y1={y(0, peak)} y2={cy} vectorEffect="non-scaling-stroke" />
                <Marker shape={generationShape(p.generation)} cx={cx} cy={cy} />
              </g>
            )
          })}
        </g>
        <rect x={x(cur.startNs, durationNs) - 6} y={MT} width={12} height={H - MT - MB} fill="none" stroke="currentColor"
          strokeDasharray="2 2" className="gc-cursor" />
      </svg>
      <p className="muted gc-legend" role="status">
        <span aria-hidden="true">● gen0 &nbsp; ■ gen1 &nbsp; ▲ gen2 &nbsp; ◆ other</span>
        <span className="sr-only">Shapes: circle gen0, square gen1, triangle gen2, diamond other.</span>
        {' '}Current: {generationLabel(cur.generation)} at {seconds(cur.startNs)}, pause {formatPause(cur.pauseNs)}.
      </p>
    </figure>
  )
}
