import { useEffect, useMemo, useState } from 'react'
import {
  getFlame, getSession, getTop,
  type AnalysisQuery, type ApiError, type FlameNode, type ProfileRecord, type SessionRecord, type TopRow,
} from '../api'
import { ErrorNotice } from './ErrorNotice'
import { reloadPage } from './reload'
import { concentration, headline, kindGuide } from '../analysis/guide'
import { hotPath, hotPathEndsTruncated } from '../analysis/hotPath'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'
import { FidelityBadge, fidelityTip } from './FidelityBadge'
import { Findings } from './Findings'
import { PivotPanel } from './PivotPanel'
import { Term } from '../help/Term'
import { Skeleton } from './Skeleton'

type Props = {
  profileRef: string
  record: ProfileRecord
  query: AnalysisQuery
  navigate: (patch: Partial<ViewState>, mode?: 'push' | 'replace') => void
}

type Loaded = { bySelf: TopRow[]; byTotal: TopRow[]; tree: FlameNode }

const MAX_FRAMES = 25

export function Summary({ profileRef, record, query, navigate }: Props) {
  const [data, setData] = useState<Loaded | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [session, setSession] = useState<SessionRecord | null>(null)

  const { st, focus, exclude, hide, depth, tx, t0, t1 } = query
  const unit = record.sample_types[st ?? record.default_sample_type]?.unit ?? 'count'

  useEffect(() => {
    let stale = false
    const q = { st, focus, exclude, hide, depth, tx, t0, t1 }
    Promise.all([
      getTop(profileRef, { ...q, limit: 5, sort: 'self' }),
      getTop(profileRef, { ...q, limit: 5, sort: 'total' }),
      getFlame(profileRef, q),
    ]).then(
      ([bySelf, byTotal, tree]) => { if (!stale) { setData({ bySelf, byTotal, tree }); setError(null) } },
      (e: ApiError) => { if (!stale) { setData(null); setError(e) } },
    )
    return () => { stale = true }
  }, [profileRef, st, focus, exclude, hide, depth, tx, t0, t1])

  useEffect(() => {
    let stale = false
    if (record.session_id === null) {
      setSession(null)
      return
    }
    getSession(record.session_id).then(
      (s) => { if (!stale) setSession(s) },
      () => { /* warnings are optional */ },
    )
    return () => { stale = true }
  }, [record.session_id])

  const path = useMemo(() => (data ? hotPath(data.tree) : []), [data])
  const guide = kindGuide(record.kind)
  const empty = !!data && (data.tree.total <= 0 || data.bySelf.length === 0)
  const warnings = session?.warnings ?? []

  const open = (funcId: number | null) => {
    if (funcId !== null) navigate({ view: 'sandwich', func: funcId })
  }
  const pct = (v: number) => (data && data.tree.total > 0 ? formatPct((v / data.tree.total) * 100) : '0%')

  const table = (title: string, rows: TopRow[]) => (
    <section className="card" aria-label={title}>
      <h3>{title}</h3>
      <table>
        <thead>
          <tr><th>Function</th><th className="num">Self <Term k="self" icon /></th><th className="num">Total <Term k="total" icon /></th></tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.funcId}>
              <td>
                <button className="linklike mono" onClick={() => open(r.funcId)}>{r.name}</button>
              </td>
              <td className="num">{formatValue(r.selfValue, unit)} ({formatPct(r.selfPct)})</td>
              <td className="num">{formatValue(r.total, unit)} ({formatPct(r.totalPct)})</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )

  return (
    <section aria-label="Summary">
      <ErrorNotice error={error} onRetry={reloadPage} />
      {!data && !error && <Skeleton lines={5} />}
      {data && (
        <>
          <p className="muted summary-question">{guide.questions[0]}</p>
          <p className="summary-headline" role="heading" aria-level={2}>
            {headline(data.bySelf, unit)}
            {!empty && (
              <span className="muted headline-note">
                {' '}
                ({concentration(data.bySelf) === 'concentrated' ? 'concentrated in one function' : 'spread across many functions'})
              </span>
            )}
          </p>
          <p>
            <FidelityBadge fidelity={record.fidelity} /> <span className="muted">{fidelityTip(record.fidelity)}</span>
          </p>
          {warnings.length > 0 && (
            <div className="banner" role="status">
              <strong>Capture warnings</strong>
              <ul>{warnings.map((w, i) => <li key={i}>{w}</li>)}</ul>
            </div>
          )}
          <Findings profileRef={profileRef} query={query} navigate={navigate} />
          {empty ? (
            <p className="muted">No samples in this view.</p>
          ) : (
            <div className="summary-grid">
              {table('Top 5 by self', data.bySelf)}
              {table('Top 5 by total', data.byTotal)}
              <section className="card" aria-label="Hottest path">
                <h3>Hottest path</h3>
                <ol className="hotpath">
                  {path.slice(0, MAX_FRAMES).map((n, i) => (
                    <li key={i}>
                      {n.name} <span className="muted">{pct(n.total)}</span>
                    </li>
                  ))}
                </ol>
                {path.length > MAX_FRAMES && <p className="muted">… {path.length - MAX_FRAMES} more frames</p>}
                {hotPathEndsTruncated(path) && <p className="muted">(deeper frames folded)</p>}
                <div className="summary-actions">
                  <button onClick={() => navigate({ view: 'flame', hot: true })}>Show in flame</button>
                  <button onClick={() => navigate({ view: 'tree', hot: true })}>Open in call tree</button>
                </div>
              </section>
              <PivotPanel
                sessionId={record.session_id}
                functionName={data.bySelf[0]?.name ?? null}
                currentProfileId={record.id}
                navigate={navigate}
              />
              <section className="card" aria-label="What to look at">
                <h3>What to look at: {guide.title}</h3>
                <ul>{guide.questions.map((q) => <li key={q}>{q}</li>)}</ul>
                <p className="muted">How this was measured: {guide.measured}</p>
                <div className="summary-actions">
                  {guide.actions.map((a) => (
                    <button key={a.label} onClick={() => navigate(a.patch)}>{a.label}</button>
                  ))}
                </div>
              </section>
            </div>
          )}
        </>
      )}
    </section>
  )
}
