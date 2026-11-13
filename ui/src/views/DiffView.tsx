import { useEffect, useMemo, useState } from 'react'
import {
  ApiError, getDiffFlame, getDiffTop, getProfile, listProfiles,
  type DiffFlame, type DiffTop, type ProfileRecord,
} from '../api'
import { diffColor } from '../flame/diffColor'
import { FlameGraph } from '../flame/FlameGraph'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'
import { toFlameNodes, type DiffFlameNode } from './diffTree'
import { FidelityBadge } from './FidelityBadge'

type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void

function signed(v: number, unit: string): string {
  const s = formatValue(v, unit)
  return v > 0 ? `+${s}` : s
}

function delta(base: number, head: number, unit: string): string {
  const d = head - base
  const pct = base > 0 ? ` (${d >= 0 ? '+' : ''}${((d / base) * 100).toFixed(1)}%)` : base === 0 && head > 0 ? ' (new)' : ''
  return `${signed(d, unit)}${pct}`
}

export function DiffView({ state, navigate }: { state: ViewState; navigate: Nav }) {
  const headRef = state.profile!
  const baseRef = state.base
  const normalize = state.normalize
  const [head, setHead] = useState<ProfileRecord | null>(null)
  const [base, setBase] = useState<ProfileRecord | null>(null)
  const [candidates, setCandidates] = useState<ProfileRecord[]>([])
  const [stName, setStName] = useState<string | undefined>(undefined)
  const [flame, setFlame] = useState<DiffFlame | null>(null)
  const [top, setTop] = useState<DiffTop | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    getProfile(headRef).then(setHead, (e: ApiError) => setError(e.message))
    listProfiles().then(setCandidates, () => {})
  }, [headRef])
  useEffect(() => {
    setBase(null)
    if (baseRef) getProfile(baseRef).then(setBase, (e: ApiError) => setError(e.message))
  }, [baseRef])

  const { focus, exclude, hide, depth } = state
  useEffect(() => {
    if (!baseRef) return
    let stale = false
    setLoading(true)
    const q = { st: stName, normalize: (normalize ?? 'none') as 'none' | 'total', focus, exclude, hide, depth, min: 0.0005 }
    Promise.all([getDiffFlame(baseRef, headRef, q), getDiffTop(baseRef, headRef, { ...q, limit: 50 })]).then(
      ([f, t]) => {
        if (stale) return
        setFlame(f)
        setTop(t)
        setError(null)
        setLoading(false)
      },
      (e: ApiError) => {
        if (stale) return
        setFlame(null)
        setTop(null)
        setError(e.message)
        setLoading(false)
      },
    )
    return () => {
      stale = true
    }
  }, [baseRef, headRef, stName, normalize, focus, exclude, hide, depth])

  const common = useMemo(() => {
    if (!head || !base) return []
    return head.sample_types.filter((t) => base.sample_types.some((b) => b.name === t.name && b.unit === t.unit))
  }, [head, base])

  const tree = useMemo(() => (flame ? toFlameNodes(flame.root) : null), [flame])
  const unit = flame?.sampleType.unit ?? 'count'

  return (
    <section aria-label="Diff">
      <div className="toolbar">
        <button onClick={() => navigate({ view: 'summary', base: undefined, normalize: undefined }, 'push')}>
          &larr; Profile
        </button>
        <strong>Diff</strong>
        <button
          onClick={() => navigate({ profile: baseRef, base: headRef }, 'push')}
          disabled={!baseRef}
          title="Swap base and head"
        >
          Swap
        </button>
      </div>
      <div className="toolbar diff-pair">
        <div>
          <div className="muted">Base</div>
          {baseRef ? (
            <span className="mono">{base?.name ?? baseRef}</span>
          ) : (
            <select
              aria-label="Choose a base profile"
              value=""
              onChange={(e) => e.target.value && navigate({ base: e.target.value }, 'replace')}
            >
              <option value="">Choose a base profile…</option>
              {candidates
                .filter((c) => c.id !== head?.id)
                .map((c) => (
                  <option key={c.id} value={c.short_id}>{c.name} ({c.kind})</option>
                ))}
            </select>
          )}
          {base && <> <FidelityBadge fidelity={base.fidelity} /></>}
        </div>
        <div>
          <div className="muted">Head</div>
          <span className="mono">{head?.name ?? headRef}</span>
          {head && <> <FidelityBadge fidelity={head.fidelity} /></>}
        </div>
        <label className="field">
          Sample type
          <select value={stName ?? flame?.sampleType.name ?? ''} onChange={(e) => setStName(e.target.value || undefined)}>
            {common.map((t) => (
              <option key={t.name} value={t.name}>{t.name} ({t.unit})</option>
            ))}
            {common.length === 0 && <option value="">-</option>}
          </select>
        </label>
        <label className="field" style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
          <input
            type="checkbox"
            checked={normalize === 'total'}
            onChange={(e) => navigate({ normalize: e.target.checked ? 'total' : undefined }, 'replace')}
          />
          Normalize to head total
        </label>
      </div>
      {error && <div className="banner" role="alert">{error}</div>}
      {!baseRef && <p className="muted">Pick a base profile to compare against.</p>}
      {loading && !error && <p className="muted" role="status">Loading...</p>}
      <div className="diff-legend muted" aria-hidden="true">
        <span className="swatch" style={{ background: 'var(--diff-down-4)' }} /> shrank
        <span className="swatch" style={{ background: 'var(--diff-flat)' }} /> unchanged
        <span className="swatch" style={{ background: 'var(--diff-up-4)' }} /> grew
        · width = larger of base and head
      </div>
      <div style={{ opacity: loading ? 0.5 : 1 }}>
        {tree && (
          <FlameGraph
            root={tree}
            unit={unit}
            inverted={false}
            colorOf={(n) => diffColor((n as DiffFlameNode).base, (n as DiffFlameNode).head)}
            tooltipExtra={(n) => {
              const d = n as DiffFlameNode
              return (
                <div>
                  base {formatValue(d.base, unit)} → head {formatValue(d.head, unit)} · {delta(d.base, d.head, unit)}
                </div>
              )
            }}
          />
        )}
        {top && (
          <>
            <h2 className="section-title">Top changes</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Function</th>
                    <th className="num">Δ self</th><th className="num">Base self</th><th className="num">Head self</th>
                    <th className="num">Δ total</th><th className="num">Δ %</th>
                  </tr>
                </thead>
                <tbody>
                  {top.rows.map((r, i) => (
                    <tr key={`${r.name}|${r.file}|${i}`}>
                      <td className="name" title={r.file ? `${r.name}\n${r.file}` : r.name}>{r.name}</td>
                      <td className={`num ${r.deltaSelf > 0 ? 'up' : r.deltaSelf < 0 ? 'down' : ''}`}>{signed(r.deltaSelf, unit)}</td>
                      <td className="num">{formatValue(r.baseSelf, unit)}</td>
                      <td className="num">{formatValue(r.headSelf, unit)}</td>
                      <td className={`num ${r.deltaTotal > 0 ? 'up' : r.deltaTotal < 0 ? 'down' : ''}`}>{signed(r.deltaTotal, unit)}</td>
                      <td className="num">{r.baseTotal > 0 ? formatPct((r.deltaTotal / r.baseTotal) * 100) : 'new'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {top.rows.length === 0 && <p className="muted">No changes.</p>}
            </div>
          </>
        )}
      </div>
    </section>
  )
}
