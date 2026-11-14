import { useEffect, useMemo, useState } from 'react'
import { getGc, type GcEvent, type GcReport } from '../api'
import { formatValue } from '../format'
import { Tabs } from '../ui/Tabs'
import {
  formatPause, generationLabel, generationShape, rowsInRange, type GcRange,
} from '../analysis/gc'
import { AllocChart, HeapChart, PauseTimeline } from './GcCharts'
import { EmptyState } from '../ui/EmptyState'

const SHAPE_GLYPH = { circle: '●', square: '■', triangle: '▲', diamond: '◆' } as const

type SortKey = 'time' | 'pause' | 'duration' | 'heap'
type Tab = 'overview' | 'heap' | 'events'

function sorted(events: GcEvent[], key: SortKey, desc: boolean): GcEvent[] {
  const val = (e: GcEvent) =>
    key === 'time' ? e.start_ns : key === 'pause' ? e.pause_ns : key === 'duration' ? e.duration_ns : e.heap_after_bytes ?? -1
  const out = [...events].sort((a, b) => val(a) - val(b))
  return desc ? out.reverse() : out
}

function Card({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="gc-card">
      <div className="gc-card-label">{label}</div>
      <strong>{value}</strong>
      {sub && <div className="muted">{sub}</div>}
    </div>
  )
}

function EventTable({ events, total }: { events: GcEvent[]; total: number }) {
  const [key, setKey] = useState<SortKey>('time')
  const [desc, setDesc] = useState(false)
  const rows = useMemo(() => sorted(events, key, desc), [events, key, desc])
  const head = (k: SortKey, label: string) => (
    <th scope="col" aria-sort={key === k ? (desc ? 'descending' : 'ascending') : 'none'}>
      <button className="link" onClick={() => { if (key === k) setDesc(!desc); else { setKey(k); setDesc(k !== 'time') } }}>
        {label}{key === k ? (desc ? ' ▼' : ' ▲') : ''}
      </button>
    </th>
  )
  const caption = rows.length === 0
    ? 'No GCs in the selected window.'
    : total > events.length
      ? `The ${events.length} GCs with the longest pauses (of ${total.toLocaleString('en-US')}).`
      : `${rows.length} GCs.`
  return (
    <div>
      <p className="muted gc-caption">{caption}</p>
      <div className="gc-table-wrap" role="region" aria-label="Collections table" tabIndex={0}>
        <table className="gc-table">
          <thead>
            <tr>
              {head('time', 'Time')}
              <th scope="col">Gen</th>
              <th scope="col">Kind</th>
              <th scope="col">Reason</th>
              {head('pause', 'Pause')}
              {head('duration', 'Duration')}
              {head('heap', 'Heap before → after')}
            </tr>
          </thead>
          <tbody>
            {rows.map((e) => (
              <tr key={e.id}>
                <td>{(e.start_ns / 1e9).toFixed(3)} s</td>
                <td>
                  <span aria-hidden="true">{SHAPE_GLYPH[generationShape(e.generation)]} </span>
                  {generationLabel(e.generation)}
                </td>
                <td>{e.kind}</td>
                <td>{e.reason}{e.induced ? ' (explicit)' : ''}</td>
                <td>{formatPause(e.pause_ns)}</td>
                <td>{formatPause(e.duration_ns)}</td>
                <td>
                  {e.heap_before_bytes === undefined ? '–' : formatValue(e.heap_before_bytes, 'bytes')}
                  {' → '}
                  {e.heap_after_bytes === undefined ? '–' : formatValue(e.heap_after_bytes, 'bytes')}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function Report({ r }: { r: GcReport }) {
  const [range, setRange] = useState<GcRange | null>(null)
  const [tab, setTab] = useState<Tab>('overview')
  const s = r.summary
  const gens = s.byGeneration.map((g) => `${generationLabel(g.generation)} ${g.count.toLocaleString('en-US')}`).join(', ')
  if (s.totalGcs === 0) {
    return <EmptyState title="No collections happened during this capture." />
  }
  const events = rowsInRange(r.events, range)
  return (
    <>
      <Tabs label="Garbage collection views" value={tab} onValueChange={(v) => setTab(v as Tab)} between={range && (
        <p role="status" className="gc-range">
          Showing {(range[0] / 1e9).toFixed(2)} s to {(range[1] / 1e9).toFixed(2)} s{' '}
          <button onClick={() => setRange(null)}>Clear range</button>
        </p>
      )} tabs={[{ value: 'overview', label: 'Overview', panel: (
        <>
          <div className="gc-cards">
            <Card label="Collections" value={s.totalGcs.toLocaleString('en-US')} sub={gens} />
            <Card label="Longest pause" value={formatPause(s.pause.maxNs)} sub={`p99 ${formatPause(s.pause.p99Ns)}`} />
            <Card label="Paused" value={`${s.pauseSharePct.toFixed(1)}% of time`} sub={`${formatPause(s.pause.totalNs)} in total`} />
            <Card label="Peak heap" value={s.peakHeapBytes === null ? 'unknown' : formatValue(s.peakHeapBytes, 'bytes')} />
            <Card label="Allocation" value={`${formatValue(s.allocMeanBytesPerS, 'bytes')}/s`} sub={`peak ${formatValue(s.allocPeakBytesPerS, 'bytes')}/s`} />
          </div>
          {r.findings.length > 0 ? (
            <ul className="gc-findings" aria-label="GC findings">
              {r.findings.map((f) => (
                <li key={f.rule} className={`gc-finding gc-finding-${f.severity}`}>
                  <span className="gc-sev">{f.severity}</span>
                  <div>
                    <strong>{f.title}</strong>
                    <div className="muted">{f.detail}</div>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">No GC problems found.</p>
          )}
          <PauseTimeline pauses={r.series.pauses} durationNs={r.durationNs} range={range} onRange={setRange} />
          <p className="muted gc-note">
            A pause is the time application threads were stopped; it is not the GC's own running time
            (background GCs run alongside the program).
          </p>
        </>
      ) }, { value: 'heap', label: 'Heap and allocation', panel: (
        <div className="gc-charts">
          <HeapChart points={r.series.heap} durationNs={r.durationNs} range={range} />
          <AllocChart points={r.series.allocRate} durationNs={r.durationNs} range={range} />
        </div>
      ) }, { value: 'events', label: 'Collections', panel: <EventTable events={events} total={s.totalGcs} /> }]} />
      {r.notes.map((n) => <div key={n} className="muted gc-note">{n}</div>)}
    </>
  )
}

/** The GC section of a session: counts per generation, pauses, heap and allocation rate. */
export function GcView({ sessionId }: { sessionId: string }) {
  const [report, setReport] = useState<GcReport | null>(null)
  useEffect(() => {
    let live = true
    getGc(sessionId).then(
      (r) => { if (live) setReport(r.available ? r : null) },
      () => { if (live) setReport(null) },
    )
    return () => { live = false }
  }, [sessionId])
  if (!report) return null
  return (
    <section className="gc-view" aria-label="Garbage collection">
      <h3>Garbage collection <span className="muted">({report.runtime})</span></h3>
      <Report r={report} />
    </section>
  )
}
