import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ApiError, getDiffFlame, getDiffTop, getProfile, listProfiles,
  type DiffFlame, type DiffTop, type ProfileRecord,
} from '../api'
import { CheckPanel } from './CheckPanel'
import { focusPattern } from './checkLogic'
import { ErrorNotice } from './ErrorNotice'
import { reloadPage } from './reload'
import { diffColor } from '../flame/diffColor'
import { FlameGraph, type FlameExportApi } from '../flame/FlameGraph'
import { copyText, downloadBlob, downloadText, fileStem } from '../export/download'
import { flameToSvg } from '../export/svg'
import { ExportMenu, type ExportItem } from './ExportMenu'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'
import { useDocumentTitle } from '../a11y/announce'
import { viewTitle } from '../a11y/title'
import { formatRange } from '../time/range'
import { CallTree } from './CallTree'
import { diffStatus, directionMark, filterDiffTree, filterRows, toSideTree } from './diffHelpers'
import { toFlameNodes, type DiffFlameNode } from './diffTree'
import { effectiveExclude } from '../analysis/perfhawk'
import { TransformBar } from './TransformBar'
import { FidelityBadge } from './FidelityBadge'
import { Skeleton } from './Skeleton'

const PALETTE_KEY = 'perfhawk.palette'

function readPalette(): boolean {
  try {
    return localStorage.getItem(PALETTE_KEY) === 'cb'
  } catch {
    return false
  }
}

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
  const [error, setError] = useState<unknown>(null)
  useDocumentTitle(viewTitle(state, head?.name))
  const [loading, setLoading] = useState(false)
  const [tab, setTab] = useState<'flame' | 'tree'>('flame')
  const [cb, setCb] = useState(readPalette)
  const exportRef = useRef<FlameExportApi | null>(null)
  const [exportMsg, setExportMsg] = useState<string | null>(null)

  useEffect(() => {
    getProfile(headRef).then(setHead, (e: ApiError) => setError(e))
    listProfiles().then(setCandidates, () => {})
  }, [headRef])
  useEffect(() => {
    setBase(null)
    if (baseRef) getProfile(baseRef).then(setBase, (e: ApiError) => setError(e))
  }, [baseRef])

  const { focus, hide, depth, tx, diffFilter, diffSort, diffLayout, t0, t1, b0, b1 } = state
  const exclude = effectiveExclude(state.exclude, state.hidePh)
  useEffect(() => {
    if (cb) document.documentElement.setAttribute('data-palette', 'cb')
    else document.documentElement.removeAttribute('data-palette')
    return () => document.documentElement.removeAttribute('data-palette')
  }, [cb])
  const setPalette = (on: boolean) => {
    setCb(on)
    try {
      if (on) localStorage.setItem(PALETTE_KEY, 'cb')
      else localStorage.removeItem(PALETTE_KEY)
    } catch {
      /* storage unavailable: the choice lasts for this page only */
    }
  }
  useEffect(() => {
    if (!baseRef) return
    let stale = false
    setLoading(true)
    const q = { st: stName, normalize: (normalize ?? 'none') as 'none' | 'total', focus, exclude, hide, depth, tx, min: 0.0005, t0, t1, bt0: b0, bt1: b1 }
    Promise.all([getDiffFlame(baseRef, headRef, q), getDiffTop(baseRef, headRef, { ...q, limit: 50, sort: diffSort === 'percent' ? 'percent' : 'delta' })]).then(
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
        setError(e)
        setLoading(false)
      },
    )
    return () => {
      stale = true
    }
  }, [baseRef, headRef, stName, normalize, focus, exclude, hide, depth, tx, diffSort, t0, t1, b0, b1])

  const common = useMemo(() => {
    if (!head || !base) return []
    return head.sample_types.filter((t) => base.sample_types.some((b) => b.name === t.name && b.unit === t.unit))
  }, [head, base])

  const tree = useMemo(() => (flame ? toFlameNodes(flame.root) : null), [flame])
  const unit = flame?.sampleType.unit ?? 'count'
  const treeFiltered = useMemo(() => (flame ? toFlameNodes(filterDiffTree(flame.root, diffFilter)) : null), [flame, diffFilter])
  const sides = useMemo(
    () => (flame && diffLayout === 'side' ? { base: toSideTree(flame.root, 'base'), head: toSideTree(flame.root, 'head') } : null),
    [flame, diffLayout],
  )
  const rows = useMemo(() => (top ? filterRows(top.rows, diffFilter) : []), [top, diffFilter])
  const stem = fileStem(`diff-${base?.name ?? baseRef ?? 'base'}-${head?.name ?? headRef}`)
  const exportItems: ExportItem[] = []
  if (tab === 'flame' && !sides && tree) {
    exportItems.push(
      {
        label: 'Download SVG',
        run: () => downloadText(
          flameToSvg(tree, {
            width: 1200, rowHeight: 18, inverted: false, title: `Diff: ${base?.name ?? ''} → ${head?.name ?? ''}`,
            colorOf: (n) => exportRef.current?.resolveColor(n) ?? '#888',
          }),
          `${stem}.svg`, 'image/svg+xml'),
      },
      {
        label: 'Download PNG',
        run: async () => {
          const blob = await exportRef.current?.toPng()
          if (blob) downloadBlob(blob, `${stem}.png`)
          else setExportMsg('PNG export is unavailable for this view.')
        },
      },
    )
  }
  exportItems.push({
    label: 'Copy link',
    run: async () => setExportMsg((await copyText(location.href)) ? 'Link copied.' : 'Copy failed: clipboard unavailable.'),
  })
  const colorOf = (n: unknown) => diffColor((n as DiffFlameNode).base, (n as DiffFlameNode).head)
  const tooltipExtra = (n: unknown) => {
    const d = n as DiffFlameNode
    return (
      <div>
        base {formatValue(d.base, unit)} → head {formatValue(d.head, unit)} · {delta(d.base, d.head, unit)}
      </div>
    )
  }

  return (
    <section aria-label="Diff">
      <div className="toolbar">
        <button onClick={() => navigate({ view: 'summary', base: undefined, normalize: undefined }, 'push')}>
          &larr; Profile
        </button>
        <h2 className="view-title">Diff</h2>
        <button
          onClick={() => navigate({ profile: baseRef, base: headRef, t0: b0, t1: b1, b0: t0, b1: t1 }, 'push')}
          disabled={!baseRef}
          title="Swap base and head"
        >
          Swap
        </button>
      </div>
      {(t0 !== undefined || b0 !== undefined) && (
        <p className="muted" role="status">
          {t0 !== undefined && t1 !== undefined && <>Head window {formatRange({ t0, t1 })}. </>}
          {b0 !== undefined && b1 !== undefined && <>Baseline window {formatRange({ t0: b0, t1: b1 })}.</>}
        </p>
      )}
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
        <div className="tabs" role="group" aria-label="Diff filter">
          <button aria-pressed={diffFilter === 'regressions'} onClick={() => navigate({ diffFilter: diffFilter === 'regressions' ? undefined : 'regressions' }, 'replace')}>Regressions only</button>
          <button aria-pressed={diffFilter === 'improvements'} onClick={() => navigate({ diffFilter: diffFilter === 'improvements' ? undefined : 'improvements' }, 'replace')}>Improvements only</button>
        </div>
        <button aria-pressed={diffSort === 'percent'} onClick={() => navigate({ diffSort: diffSort === 'percent' ? undefined : 'percent' }, 'replace')}>Sort changes by %</button>
        <button aria-pressed={diffLayout === 'side'} onClick={() => navigate({ diffLayout: diffLayout === 'side' ? undefined : 'side' }, 'replace')}>Side by side</button>
        <label className="field" style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
          <input type="checkbox" checked={cb} onChange={(e) => setPalette(e.target.checked)} />
          Colour-blind palette
        </label>
        <div className="tabs" role="group" aria-label="Diff view">
          <button aria-pressed={tab === 'flame'} onClick={() => setTab('flame')}>Flame</button>
          <button aria-pressed={tab === 'tree'} onClick={() => setTab('tree')}>Call tree</button>
        </div>
      </div>
      {baseRef && head && base && (
        <details className="check-panel">
          <summary>Regression check</summary>
          <CheckPanel
            head={headRef}
            base={baseRef}
            sampleType={stName}
            onFocusFunction={(name) => navigate({ focus: focusPattern(name) }, 'replace')}
          />
        </details>
      )}
      <TransformBar tx={tx} onChange={(next) => navigate({ tx: next }, 'push')} />
      <div className="toolbar"><ExportMenu items={exportItems} /></div>
      {exportMsg && <p className="muted" role="status">{exportMsg}</p>}
      <ErrorNotice error={error} onRetry={reloadPage} />
      {!baseRef && <p className="muted">Pick a base profile to compare against.</p>}
      {loading && !error && <Skeleton />}
      <div className="diff-legend muted" aria-hidden="true">
        <span className="swatch" style={{ background: 'var(--diff-down-4)' }} /> ▼ shrank
        <span className="swatch" style={{ background: 'var(--diff-flat)' }} /> unchanged
        <span className="swatch" style={{ background: 'var(--diff-up-4)' }} /> ▲ grew
        · width = larger of base and head
      </div>
      <div style={{ opacity: loading ? 0.5 : 1 }}>
        {tab === 'flame' && !sides && tree && (
          <FlameGraph root={tree} unit={unit} inverted={false} colorOf={colorOf} tooltipExtra={tooltipExtra} exportRef={exportRef} />
        )}
        {tab === 'flame' && sides && (
          <div className="diff-sides">
            <div>
              <div className="section-title">Base</div>
              <FlameGraph root={sides.base} unit={unit} inverted={false} colorOf={colorOf} tooltipExtra={tooltipExtra} />
            </div>
            <div>
              <div className="section-title">Head</div>
              <FlameGraph root={sides.head} unit={unit} inverted={false} colorOf={colorOf} tooltipExtra={tooltipExtra} />
            </div>
          </div>
        )}
        {tab === 'tree' && treeFiltered && (
          <CallTree
            root={treeFiltered}
            unit={unit}
            hot={false}
            onSelectFunc={() => {}}
            extra={{ header: 'Δ (head − base)', cell: (n) => signed((n as DiffFlameNode).head - (n as DiffFlameNode).base, unit) }}
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
                    <th className="num">Δ total</th><th className="num">Δ %</th><th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r, i) => (
                    <tr key={`${r.name}|${r.file}|${i}`}>
                      <td className="name" title={r.file ? `${r.name}\n${r.file}` : r.name}>{r.name}</td>
                      <td className={`num ${r.deltaSelf > 0 ? 'up' : r.deltaSelf < 0 ? 'down' : ''}`}><span aria-hidden="true">{directionMark(r.deltaSelf)}</span> {signed(r.deltaSelf, unit)}</td>
                      <td className="num">{formatValue(r.baseSelf, unit)}</td>
                      <td className="num">{formatValue(r.headSelf, unit)}</td>
                      <td className={`num ${r.deltaTotal > 0 ? 'up' : r.deltaTotal < 0 ? 'down' : ''}`}><span aria-hidden="true">{directionMark(r.deltaTotal)}</span> {signed(r.deltaTotal, unit)}</td>
                      <td className="num">{r.baseTotal > 0 ? formatPct((r.deltaTotal / r.baseTotal) * 100) : ''}</td>
                      <td>{(() => { const st = diffStatus(r.baseTotal, r.headTotal); return st === 'new' || st === 'gone' ? st : '' })()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {rows.length === 0 && <p className="muted">No changes.</p>}
            </div>
          </>
        )}
      </div>
    </section>
  )
}
