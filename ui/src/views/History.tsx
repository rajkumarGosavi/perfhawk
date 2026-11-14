import { useEffect, useMemo, useRef, useState } from 'react'
import {
  getHistory, getSeries, getTop, listProfiles,
  type ApiError, type HistoryResponse, type SeriesInfo,
} from '../api'
import { ErrorNotice } from './ErrorNotice'
import { reloadPage } from './reload'
import { deltaFromPrevious, linePath, nearest, scale, type Metric } from '../history/chart'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'

type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void

const W = 640
const H = 200
const PAD = 24

export function History({ state, navigate }: { state: ViewState; navigate: Nav }) {
  const [series, setSeries] = useState<SeriesInfo[] | null>(null)
  const [data, setData] = useState<HistoryResponse | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [metric, setMetric] = useState<Metric>('value')
  const [suggestions, setSuggestions] = useState<string[]>([])
  const [focusIdx, setFocusIdx] = useState<number | null>(null)
  const [fnText, setFnText] = useState(state.fn ?? '')
  const pointRefs = useRef(new Map<number, SVGCircleElement>())

  useEffect(() => setFnText(state.fn ?? ''), [state.fn])
  useEffect(() => {
    let stale = false
    getSeries().then(
      (s) => { if (!stale) setSeries(s) },
      (e: ApiError) => { if (!stale) setError(e) },
    )
    return () => { stale = true }
  }, [])

  const current = series?.find((s) => s.key === state.series)
  const kind = state.kind && current?.kinds.includes(state.kind) ? state.kind : current?.kinds[0]
  const fn = state.fn

  // Suggest the latest session's hottest functions for this kind.
  useEffect(() => {
    let stale = false
    setSuggestions([])
    if (!current || !kind) return
    listProfiles()
      .then(async (all) => {
        const latest = all
          .filter((p) => p.session_id === current.latestSessionId && p.kind === kind)
          .at(0)
        if (!latest) return []
        return (await getTop(latest.short_id, { limit: 10 })).map((r) => r.name)
      })
      .then((names) => { if (!stale) setSuggestions(names) }, () => {})
    return () => { stale = true }
  }, [current, kind])

  useEffect(() => {
    let stale = false
    setData(null)
    setFocusIdx(null)
    if (!state.series || !kind || !fn) return
    getHistory({ series: state.series, kind, func: fn }).then(
      (d) => { if (!stale) { setData(d); setError(null) } },
      (e: ApiError) => { if (!stale) setError(e) },
    )
    return () => { stale = true }
  }, [state.series, kind, fn])

  const pts = useMemo(() => (data ? scale(data.points, W - 2 * PAD, H - 2 * PAD, metric) : []), [data, metric])
  const unit = data?.sampleType?.unit ?? 'count'
  // Roving tabindex: the focused point, else the most recent found one.
  const lastFound = nearest(pts, Number.MAX_SAFE_INTEGER)
  const tabbable = focusIdx ?? lastFound
  const foundCount = data ? data.points.filter((p) => p.found).length : 0

  // Moving focus with the keyboard must move real focus, not just the tab stop.
  useEffect(() => {
    if (focusIdx !== null) pointRefs.current.get(focusIdx)?.focus()
  }, [focusIdx])

  const open = (i: number) => {
    const p = data?.points[i]
    if (p?.found) navigate({ profile: p.profileId.slice(0, 12), view: 'flame', search: fn }, 'push', true)
  }
  const diffWithPrevious = (i: number) => {
    if (!data) return
    let prev = i - 1
    while (prev >= 0 && !data.points[prev].found) prev--
    if (prev < 0 || !data.points[i].found) return
    navigate({ profile: data.points[i].profileId.slice(0, 12), base: data.points[prev].profileId.slice(0, 12), view: 'diff' }, 'push', true)
  }

  const onKey = (e: React.KeyboardEvent) => {
    if (!data) return
    const found = pts.filter((p) => p.found).map((p) => p.index)
    if (found.length === 0) return
    const cur = focusIdx ?? found[found.length - 1]
    const pos = found.indexOf(cur)
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault()
      setFocusIdx(found[Math.min(found.length - 1, Math.max(0, pos + (e.key === 'ArrowLeft' ? -1 : 1)))])
    } else if (e.key === 'Enter') {
      e.preventDefault()
      open(cur)
    }
  }

  const commitFn = () => {
    const v = fnText.trim()
    if (v !== (state.fn ?? '')) navigate({ fn: v || undefined }, 'replace')
  }

  return (
    <section aria-label="History">
      <div className="toolbar">
        <button onClick={() => navigate({}, 'push', true)}>&larr; Home</button>
        <h2 className="view-title">History</h2>
        <strong>History</strong>
        <label className="field">
          Series
          <select
            value={state.series ?? ''}
            onChange={(e) => navigate({ series: e.target.value || undefined, kind: undefined }, 'replace')}
          >
            <option value="">{series && series.length === 0 ? 'no sessions yet' : 'choose a series'}</option>
            {series?.map((s) => (
              <option key={s.key} value={s.key}>{s.key} ({s.sessionCount})</option>
            ))}
          </select>
        </label>
        <label className="field">
          Kind
          <select
            value={kind ?? ''}
            disabled={!current}
            onChange={(e) => navigate({ kind: e.target.value || undefined }, 'replace')}
          >
            {current?.kinds.map((k) => <option key={k} value={k}>{k}</option>)}
          </select>
        </label>
        <label className="field">
          Function
          <input
            list="history-functions"
            value={fnText}
            disabled={!current}
            placeholder="exact function name"
            onChange={(e) => setFnText(e.target.value)}
            onBlur={commitFn}
            onKeyDown={(e) => { if (e.key === 'Enter') commitFn() }}
          />
          <datalist id="history-functions">
            {suggestions.map((n) => <option key={n} value={n} />)}
          </datalist>
        </label>
        <div className="tabs" role="group" aria-label="Metric">
          <button aria-pressed={metric === 'value'} onClick={() => setMetric('value')}>Value</button>
          <button aria-pressed={metric === 'pct'} onClick={() => setMetric('pct')}>% of total</button>
        </div>
      </div>
      <ErrorNotice error={error} onRetry={reloadPage} />
      {!state.series && <p className="muted">Pick a series to see how a function changes across sessions. Tag recordings with <code>--tag series:name</code> to group them.</p>}
      {state.series && !fn && <p className="muted">Pick or type a function name.</p>}
      {data && data.points.length === 0 && <p className="muted">No sessions of this series captured a {kind} profile.</p>}
      {data && data.points.length > 0 && (
        <>
          {foundCount < 2 && <p className="muted">Need at least 2 sessions to show a trend.</p>}
          <svg
            className="history-chart"
            viewBox={`0 0 ${W} ${H}`}
            role="group"
            aria-label={`${fn} across ${data.points.length} sessions. Arrow keys move between points, Enter opens the session.`}
            onKeyDown={onKey}
          >
            <g transform={`translate(${PAD} ${PAD})`}>
              <path d={linePath(pts)} fill="none" stroke="currentColor" strokeWidth={2} />
              {pts.map((p) => {
                const pt = data.points[p.index]
                return p.found ? (
                  <circle
                    key={p.index}
                    ref={(el) => { if (el) pointRefs.current.set(p.index, el); else pointRefs.current.delete(p.index) }}
                    cx={p.x}
                    cy={p.y}
                    r={p.index === focusIdx ? 6 : 4}
                    className="history-point"
                    tabIndex={p.index === tabbable ? 0 : -1}
                    role="button"
                    aria-label={`${new Date(pt.startedAtUnixMs).toLocaleString()}: ${formatValue(pt.value, unit)}, ${formatPct(pt.pct)} of total`}
                    onFocus={() => setFocusIdx(p.index)}
                    onClick={() => open(p.index)}
                  />
                ) : (
                  <text key={p.index} x={p.x} y={H - 2 * PAD} textAnchor="middle" className="muted">×</text>
                )
              })}
            </g>
          </svg>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>Session</th><th>Started</th><th className="num">Value</th><th className="num">% of total</th><th className="num">Change vs previous</th><th aria-label="Actions" /></tr>
              </thead>
              <tbody>
                {data.points.map((p, i) => {
                  const d = deltaFromPrevious(data.points, i, metric)
                  return (
                    <tr key={p.sessionId}>
                      <td className="mono">{p.shortId}</td>
                      <td>{new Date(p.startedAtUnixMs).toLocaleString()}</td>
                      {p.found ? (
                        <>
                          <td className="num">{formatValue(p.value, unit)}</td>
                          <td className="num">{formatPct(p.pct)}</td>
                          <td className="num">
                            {d === null ? '–' : `${d > 0 ? '+' : ''}${metric === 'pct' ? formatPct(d) : formatValue(d, unit)}`}
                          </td>
                          <td>
                            <button onClick={() => open(i)}>Open</button>{' '}
                            <button onClick={() => diffWithPrevious(i)} disabled={i === 0}>Diff with previous</button>
                          </td>
                        </>
                      ) : (
                        <td colSpan={4} className="muted">{p.skipped ?? 'function not present in this session'}</td>
                      )}
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  )
}
