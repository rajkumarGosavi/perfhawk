import { useState } from 'react'
import type { GcHeapPoint, GcPausePoint, GcRatePoint } from '../api'
import { formatValue } from '../format'
import {
  formatPause, generationLabel, generationShape, rangeAround, stackedHeap, type GcRange, type GcShape,
} from '../analysis/gc'

const W = 600
const H = 120
const PAD = 4

const x = (tNs: number, durationNs: number) => PAD + (durationNs > 0 ? (tNs / durationNs) * (W - 2 * PAD) : 0)
const y = (v: number, max: number) => H - PAD - (max > 0 ? (v / max) * (H - 2 * PAD) : 0)

function seconds(ns: number): string {
  return `${(ns / 1e9).toFixed(2)} s`
}

function RangeBand({ range, durationNs }: { range: GcRange | null; durationNs: number }) {
  if (!range) return null
  const x0 = x(range[0], durationNs)
  return <rect x={x0} y={0} width={Math.max(2, x(range[1], durationNs) - x0)} height={H} className="gc-band" />
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
    const top = st.rows.map((r) => `${x(r.tNs, durationNs)},${y(r.cum[i], peak)}`)
    const bottom = st.rows.map((r) => `${x(r.tNs, durationNs)},${y(i === 0 ? 0 : r.cum[i - 1], peak)}`).reverse()
    return { key: k, d: `M${[...top, ...bottom].join(' L')} Z` }
  })
  const line = points.map((p) => `${x(p.t_ns, durationNs)},${y(p.total_bytes, peak)}`).join(' L')
  const last = st.rows[st.rows.length - 1]
  return (
    <figure className="gc-chart">
      <figcaption>Heap over time <span className="muted">(largest sample {formatValue(peak, 'bytes')})</span></figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`Heap size over time, largest sample ${formatValue(peak, 'bytes')}. A data table follows.`}>
        <RangeBand range={range} durationNs={durationNs} />
        {layers.map((l, i) => (
          <path key={l.key} d={l.d} fill="currentColor" fillOpacity={0.12 + i * 0.12} stroke="currentColor"
            strokeWidth={1} strokeDasharray={i % 2 ? '4 2' : undefined} vectorEffect="non-scaling-stroke" />
        ))}
        <path d={`M${line}`} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
        {last && st.keys.map((k, i) => (
          <text key={k} x={W - PAD - 2} y={y((last.cum[i] + (i === 0 ? 0 : last.cum[i - 1])) / 2, peak)}
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
  if (points.length === 0) return <p className="muted">No allocation data was captured.</p>
  const peak = Math.max(...points.map((p) => p.bytesPerS), 1)
  const line = points.map((p) => `${x(p.tNs, durationNs)},${y(p.bytesPerS, peak)}`).join(' L')
  return (
    <figure className="gc-chart">
      <figcaption>Allocation rate <span className="muted">(peak {formatValue(peak, 'bytes')}/s)</span></figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`Allocation rate over time, peak ${formatValue(peak, 'bytes')} per second. A data table follows.`}>
        <RangeBand range={range} durationNs={durationNs} />
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
        Pauses <span className="muted">(longest {formatPause(peak)}; shape shows generation)</span>
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
        <RangeBand range={shown} durationNs={durationNs} />
        <g fill="currentColor" stroke="currentColor" strokeWidth={1}>
          {ordered.map((p) => {
            const cx = x(p.startNs, durationNs)
            const cy = y(p.pauseNs, peak)
            return (
              <g key={p.id}>
                <line x1={cx} x2={cx} y1={H - PAD} y2={cy} vectorEffect="non-scaling-stroke" />
                <Marker shape={generationShape(p.generation)} cx={cx} cy={cy} />
              </g>
            )
          })}
        </g>
        <rect x={x(cur.startNs, durationNs) - 6} y={0} width={12} height={H} fill="none" stroke="currentColor"
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
