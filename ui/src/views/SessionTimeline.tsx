import { useEffect, useState, type ReactNode } from 'react'
import {
  getSessionCounters, getSessionTimeline, type ApiError, type CounterFinding, type CounterRow,
  type SessionTimeline as TimelineData,
} from '../api'
import { formatValue } from '../format'
import { formatRange, type Range } from '../time/range'
import { fillGaps, indexToRange, peak, toPath } from '../time/sparkline'

type Props = {
  sessionId: string
  /** Opens a profile's flame view limited to a window. */
  onOpenWindow: (shortId: string, range: Range) => void
}

const W = 600
const H = 36

/** Counters shown without "Show all": the ones that explain queue build-up under load. */
export const DEFAULT_COUNTERS = new Set([
  'threadpool-queue-length',
  'threadpool-thread-count',
  'cpu-usage',
  'gc-heap-size',
  'monitor-lock-contention-count',
  'exception-count',
  '/sched/goroutines:goroutines',
  '/sched/goroutines/runnable:goroutines',
  'sched-latency-p99-ms',
])

function formatCounter(v: number, unit: string): string {
  if (unit === 'B') return formatValue(v, 'bytes')
  const n = Number.isInteger(v) ? v.toLocaleString() : v.toFixed(2)
  return unit ? `${n} ${unit}` : n
}

/** One sparkline per kind with time data, then runtime counters; one selection is shared by every row. */
export function SessionTimeline({ sessionId, onOpenWindow }: Props) {
  const [data, setData] = useState<TimelineData | null>(null)
  const [findings, setFindings] = useState<CounterFinding[]>([])
  const [selected, setSelected] = useState<number | null>(null)
  const [showAll, setShowAll] = useState(false)

  useEffect(() => {
    let stale = false
    getSessionTimeline(sessionId, 200).then(
      (d) => {
        if (stale) return
        setData(d)
        setFindings([])
        if ((d.counters ?? []).length > 0) {
          getSessionCounters(sessionId).then(
            (r) => { if (!stale && r.available) setFindings(r.findings) },
            (_e: ApiError) => {},
          )
        }
      },
      (_e: ApiError) => { if (!stale) setData(null) },
    )
    return () => { stale = true }
  }, [sessionId])

  const counters = data?.counters ?? []
  if (!data || (data.series.length === 0 && counters.length === 0)) return null
  const points = data.series[0]?.values.length ?? counters[0].values.length
  const range: Range | null = selected === null ? null : indexToRange(selected, points, data.durationMs)
  const shown = showAll ? counters : counters.filter((c) => DEFAULT_COUNTERS.has(c.name))
  const hidden = counters.length - counters.filter((c) => DEFAULT_COUNTERS.has(c.name)).length

  const sparkline = (label: string, values: number[], extra?: ReactNode) => {
    const pk = peak(values)
    return (
      <svg
        className="sparkline"
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        role="button"
        tabIndex={0}
        aria-label={`${label} over time. Click or press Enter to select the window around the peak.`}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect()
          setSelected(Math.min(points - 1, Math.max(0, Math.floor(((e.clientX - r.left) / r.width) * points))))
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            setSelected(pk ? pk.index : 0)
          } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
            e.preventDefault()
            setSelected((i) => Math.min(points - 1, Math.max(0, (i ?? pk?.index ?? 0) + (e.key === 'ArrowLeft' ? -1 : 1))))
          }
        }}
      >
        {selected !== null && (
          <rect x={(selected / points) * W} y={0} width={Math.max(W / points, 2)} height={H} className="timeline-band" />
        )}
        <path d={toPath(values, W, H - 2)} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
        {extra}
      </svg>
    )
  }

  const counterRow = (c: CounterRow) => {
    const values = fillGaps(c.values)
    const pk = c.values.some((v) => v !== null) ? peak(values) : null
    const markers = c.events.map((e, i) => (
      <rect key={i} className="timeline-marker" x={(e.index / points) * W} y={0} width={Math.max(W / points, 2)} height={H}>
        <title>{e.detail}</title>
      </rect>
    ))
    return (
      <div key={c.name} className="timeline-row">
        <div className="timeline-label">
          <strong>{c.displayName}</strong>
          <span className="muted"> peak {pk ? formatCounter(pk.value, c.unit) : '–'}</span>
        </div>
        {sparkline(c.displayName, values, markers)}
        <span />
      </div>
    )
  }

  return (
    <section className="session-timeline" aria-label="Session timeline">
      <h3>Timeline <span className="muted">({(data.durationMs / 1000).toFixed(1)} s)</span></h3>
      {findings.length > 0 && (
        <ul className="counter-findings" aria-label="Counter findings">
          {findings.map((f) => (
            <li key={f.id + f.title}>
              <strong>{f.title}</strong>: {f.detail} <span className="muted">(threshold: {f.threshold})</span>
            </li>
          ))}
        </ul>
      )}
      {range && <p className="muted" role="status">Selected window {formatRange(range)}</p>}
      {data.series.map((s) => {
        const pk = peak(s.values)
        return (
          <div key={s.profileId} className="timeline-row">
            <div className="timeline-label">
              <strong>{s.kind}</strong>
              <span className="muted"> peak {pk ? formatValue(pk.value, s.sampleType.unit) : '–'}</span>
            </div>
            {sparkline(s.kind, s.values)}
            <button disabled={!range} onClick={() => range && onOpenWindow(s.shortId, range)}>
              Open window in {s.kind}
            </button>
          </div>
        )
      })}
      {shown.map(counterRow)}
      {hidden > 0 && (
        <button className="linklike" onClick={() => setShowAll((v) => !v)}>
          {showAll ? 'Show fewer counters' : `Show all ${counters.length} counters`}
        </button>
      )}
    </section>
  )
}
