import { useEffect, useState } from 'react'
import { getSessionTimeline, type ApiError, type SessionTimeline as TimelineData } from '../api'
import { formatValue } from '../format'
import { formatRange, type Range } from '../time/range'
import { indexToRange, peak, toPath } from '../time/sparkline'

type Props = {
  sessionId: string
  /** Opens a profile's flame view limited to a window. */
  onOpenWindow: (shortId: string, range: Range) => void
}

const W = 600
const H = 36

/** One sparkline per kind with time data; one selection is shared by every row. */
export function SessionTimeline({ sessionId, onOpenWindow }: Props) {
  const [data, setData] = useState<TimelineData | null>(null)
  const [selected, setSelected] = useState<number | null>(null)

  useEffect(() => {
    let stale = false
    getSessionTimeline(sessionId, 200).then(
      (d) => { if (!stale) setData(d) },
      (_e: ApiError) => { if (!stale) setData(null) },
    )
    return () => { stale = true }
  }, [sessionId])

  if (!data || data.series.length === 0) return null
  const points = data.series[0].values.length
  const range: Range | null = selected === null ? null : indexToRange(selected, points, data.durationMs)

  return (
    <section className="session-timeline" aria-label="Session timeline">
      <h3>Timeline <span className="muted">({(data.durationMs / 1000).toFixed(1)} s)</span></h3>
      {range && <p className="muted" role="status">Selected window {formatRange(range)}</p>}
      {data.series.map((s) => {
        const pk = peak(s.values)
        return (
          <div key={s.profileId} className="timeline-row">
            <div className="timeline-label">
              <strong>{s.kind}</strong>
              <span className="muted"> peak {pk ? formatValue(pk.value, s.sampleType.unit) : '–'}</span>
            </div>
            <svg
              className="sparkline"
              viewBox={`0 0 ${W} ${H}`}
              preserveAspectRatio="none"
              role="button"
              tabIndex={0}
              aria-label={`${s.kind} over time. Click or press Enter to select the window around the peak.`}
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
              <path d={toPath(s.values, W, H - 2)} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
            </svg>
            <button disabled={!range} onClick={() => range && onOpenWindow(s.shortId, range)}>
              Open window in {s.kind}
            </button>
          </div>
        )
      })}
    </section>
  )
}
