import { useEffect, useState } from 'react'
import { getHeap, type HeapSummary } from '../api'
import { formatValue } from '../format'

function Card({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="gc-card">
      <div className="gc-card-label">{label}</div>
      <strong>{value}</strong>
      {sub && <div className="muted">{sub}</div>}
    </div>
  )
}

/** The heap snapshot section of a session: totals and the types holding the most memory. */
export function HeapView({ sessionId }: { sessionId: string }) {
  const [heap, setHeap] = useState<HeapSummary | null>(null)
  useEffect(() => {
    let live = true
    getHeap(sessionId).then(
      (r) => { if (live) setHeap(r.available ? r : null) },
      () => { if (live) setHeap(null) },
    )
    return () => { live = false }
  }, [sessionId])
  if (!heap) return null
  const more = heap.typeCount - heap.types.length
  return (
    <section className="gc-view" aria-label="Heap snapshot">
      <h3>Heap snapshot <span className="muted">({heap.runtime})</span></h3>
      {!heap.complete && (
        <p className="muted gc-note">
          Incomplete snapshot: the runtime dropped events (large heap or timeout), so counts and sizes are lower bounds.
        </p>
      )}
      <div className="gc-cards">
        <Card label="Live objects" value={heap.objects.toLocaleString('en-US')} sub={`${heap.references.toLocaleString('en-US')} references`} />
        <Card label="Heap size" value={formatValue(heap.totalBytes, 'bytes')} sub="shallow, live objects" />
        <Card label="Types" value={heap.typeCount.toLocaleString('en-US')} />
        <Card
          label="Roots"
          value={heap.roots.reduce((n, r) => n + r.count, 0).toLocaleString('en-US')}
          sub={heap.roots.map((r) => `${r.kind} ${r.count}`).join(', ')}
        />
      </div>
      <p className="muted gc-caption">
        Shallow size is the objects&apos; own bytes, not what they reference.
        {more > 0 ? ` Showing the ${heap.types.length} largest of ${heap.typeCount.toLocaleString('en-US')} types.` : ''}
      </p>
      <div className="gc-table-wrap" role="region" aria-label="Types table" tabIndex={0}>
        <table className="gc-table">
          <thead>
            <tr>
              <th scope="col">Type</th>
              <th scope="col">Count</th>
              <th scope="col">Shallow size</th>
              <th scope="col">Share</th>
            </tr>
          </thead>
          <tbody>
            {heap.types.map((t) => (
              <tr key={t.name}>
                <td className="mono">{t.name}</td>
                <td>{t.count.toLocaleString('en-US')}</td>
                <td>{formatValue(t.shallowBytes, 'bytes')}</td>
                <td>{t.sharePct.toFixed(1)}%</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
