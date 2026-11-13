import { useEffect, useRef, useState } from 'react'
import {
  ApiError, getFlame, getGraph, getGroups, getHeatmap, getProfile, getSandwich, getTop, listProfiles,
  type CallGraph, type FlameNode, type GroupRow, type Heatmap, type ProfileRecord, type Sandwich as SandwichData, type TopRow,
} from '../api'
import { hotPath } from '../analysis/hotPath'
import { SHORTCUTS, keyAction } from '../analysis/keys'
import { addTransform, decodeTx, encodeTx, type Transform } from '../analysis/transforms'
import { FlameGraph, type FlameExportApi } from '../flame/FlameGraph'
import { toCollapsed } from '../export/collapsed'
import { toCsv } from '../export/csv'
import { copyText, downloadBlob, downloadText, fileStem } from '../export/download'
import { flameToSvg } from '../export/svg'
import { ExportMenu, type ExportItem } from './ExportMenu'
import type { ViewName, ViewState } from '../url'
import { CallTree } from './CallTree'
import { formatRange } from '../time/range'
import { GraphView } from './GraphView'
import { HeatMap } from './HeatMap'
import { Groups } from './Groups'
import { PivotPanel } from './PivotPanel'
import { Sandwich } from './Sandwich'
import { Summary } from './Summary'
import { FidelityBadge } from './FidelityBadge'
import { TopTable } from './TopTable'
import { TransformBar } from './TransformBar'

type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void
type Data =
  | { kind: 'flame'; tree: FlameNode }
  | { kind: 'top'; rows: TopRow[] }
  | { kind: 'both'; tree: FlameNode; rows: TopRow[] }
  | { kind: 'sandwich'; data: SandwichData }
  | { kind: 'groups'; rows: GroupRow[] }
  | { kind: 'graph'; graph: CallGraph }
  | { kind: 'heat'; heat: Heatmap }

const VIEW_LABELS: [ViewName, string][] = [
  ['summary', 'Summary'], ['flame', 'Flame'], ['icicle', 'Icicle'], ['tree', 'Call tree'],
  ['top', 'Top'], ['both', 'Both'], ['sandwich', 'Sandwich'], ['groups', 'Groups'], ['graph', 'Graph'], ['time', 'Time'],
]
const defaultGroupBy = (kind: string): 'stack' | 'site' => (kind === 'lock' || kind === 'blocking' ? 'site' : 'stack')
const HOT_VIEWS: ViewName[] = ['flame', 'icicle', 'tree', 'both']

// Local text state, committed to the URL 300 ms after typing stops.
function useDebounced(committed: string, commit: (v: string) => void) {
  const [text, setText] = useState(committed)
  // Follow external changes (back/forward).
  useEffect(() => setText(committed), [committed])
  useEffect(() => {
    if (text === committed) return
    const t = setTimeout(() => commit(text), 300)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text])
  return [text, setText] as const
}

export function ProfileView({ state, navigate }: { state: ViewState; navigate: Nav }) {
  const ref = state.profile!
  const [record, setRecord] = useState<ProfileRecord | null>(null)
  const [data, setData] = useState<Data | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [others, setOthers] = useState<ProfileRecord[]>([])
  const [helpOpen, setHelpOpen] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)
  const exportRef = useRef<FlameExportApi | null>(null)
  const [exportMsg, setExportMsg] = useState<string | null>(null)

  const replace = (patch: Partial<ViewState>) => navigate(patch, 'replace')
  const [focus, setFocus] = useDebounced(state.focus ?? '', (v) => replace({ focus: v || undefined }))
  const [exclude, setExclude] = useDebounced(state.exclude ?? '', (v) => replace({ exclude: v || undefined }))
  const [hide, setHide] = useDebounced(state.hide ?? '', (v) => replace({ hide: v || undefined }))
  const [search, setSearch] = useDebounced(state.search ?? '', (v) => replace({ search: v || undefined }))

  useEffect(() => {
    getProfile(ref).then(setRecord, (e: ApiError) => setError(e.message))
  }, [ref])
  // Profiles of the same kind that this one can be compared with, newest first.
  useEffect(() => {
    if (!record) return
    listProfiles().then(
      (all) => setOthers(all.filter((p) => p.kind === record.kind && p.id !== record.id)),
      () => setOthers([]),
    )
  }, [record])

  const st = state.st ?? record?.default_sample_type
  const unit = record && st !== undefined ? record.sample_types[st]?.unit ?? 'count' : 'count'
  const { view, func, focus: qf, exclude: qe, hide: qh, depth, tx, t0, t1 } = state
  const hasTime = !!record?.has_timeline
  const groupBy = state.groupBy ?? defaultGroupBy(record?.kind ?? '')
  const sort = state.sort ?? 'self'

  useEffect(() => {
    if (!record) return
    if (view === 'summary' || (view === 'sandwich' && func === undefined) || (view === 'time' && !record.has_timeline)) {
      setData(null)
      setLoading(false)
      return
    }
    let stale = false
    setLoading(true)
    const q = { st, focus: qf, exclude: qe, hide: qh, depth, tx, t0, t1 }
    const p: Promise<Data> =
      view === 'time'
        ? getHeatmap(ref, { st, focus: qf, exclude: qe, hide: qh, tx, cols: 600 }).then((heat) => ({ kind: 'heat', heat }))
        : view === 'graph'
        ? getGraph(ref, { ...q, nodes: state.graphNodes ?? 80 }).then((graph) => ({ kind: 'graph', graph }))
        : view === 'groups'
        ? getGroups(ref, { ...q, by: groupBy, limit: 500 }).then((rows) => ({ kind: 'groups', rows }))
        : view === 'top'
        ? getTop(ref, { ...q, limit: 500, sort }).then((rows) => ({ kind: 'top', rows }))
        : view === 'both'
          ? Promise.all([getFlame(ref, q), getTop(ref, { ...q, limit: 500, sort })]).then(([tree, rows]) => ({
              kind: 'both', tree, rows,
            }))
          : view === 'sandwich'
          ? getSandwich(ref, func!, q).then((data) => ({ kind: 'sandwich', data }))
          : getFlame(ref, q).then((tree) => ({ kind: 'flame', tree }))
    p.then(
      (d) => { if (!stale) { setData(d); setError(null); setLoading(false) } },
      (e: ApiError) => { if (!stale) { setData(null); setError(e.message); setLoading(false) } },
    )
    return () => { stale = true }
  }, [record, ref, view, func, st, qf, qe, qh, sort, depth, tx, groupBy, state.graphNodes, t0, t1])

  const pushView = (patch: Partial<ViewState>) => navigate(patch, 'push')
  const hot = !!state.hot
  const onTransform = (t: Transform) => navigate({ tx: encodeTx(addTransform(decodeTx(tx), t)) }, 'push')
  const toggleHot = () => navigate({ hot: hot ? undefined : true }, 'replace')

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const a = keyAction(e)
      if (!a) return
      if (a.type === 'view') navigate({ view: a.view }, 'push')
      else if (a.type === 'hot') navigate({ hot: hot ? undefined : true }, 'replace')
      else if (a.type === 'search') {
        e.preventDefault()
        searchRef.current?.focus()
      } else setHelpOpen((o) => !o)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigate, hot])

  useEffect(() => {
    if (!helpOpen) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setHelpOpen(false) }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [helpOpen])

  const stem = fileStem(record?.name ?? ref)
  const say = (m: string) => setExportMsg(m)
  const flameItems = (tree: FlameNode, inverted: boolean): ExportItem[] => [
    {
      label: 'Download SVG',
      run: () => {
        const api = exportRef.current
        const svg = flameToSvg(tree, {
          width: 1200, rowHeight: 18, inverted, title: record?.name ?? ref,
          colorOf: (n) => api?.resolveColor(n) ?? '#e0705f',
        })
        downloadText(svg, `${stem}.svg`, 'image/svg+xml')
      },
    },
    {
      label: 'Download PNG',
      run: async () => {
        const blob = await exportRef.current?.toPng()
        if (blob) downloadBlob(blob, `${stem}.png`)
        else say('PNG export is unavailable for this view.')
      },
    },
    {
      label: 'Copy collapsed stacks',
      run: async () => say((await copyText(toCollapsed(tree))) ? 'Collapsed stacks copied.' : 'Copy failed: clipboard unavailable.'),
    },
  ]
  const linkItem: ExportItem = {
    label: 'Copy link',
    run: async () => say((await copyText(location.href)) ? 'Link copied.' : 'Copy failed: clipboard unavailable.'),
  }
  const exportItems: ExportItem[] = []
  if (data && (data.kind === 'flame' || data.kind === 'both') && (view === 'flame' || view === 'icicle' || view === 'both')) {
    exportItems.push(...flameItems(data.tree, view === 'icicle'))
  }
  if (data?.kind === 'top' && view === 'top') {
    exportItems.push({
      label: 'Download CSV',
      run: () => downloadText(
        toCsv(['function', 'file', 'self', 'total', 'self_pct', 'total_pct'],
          data.rows.map((r) => [r.name, r.file, r.selfValue, r.total, r.selfPct, r.totalPct])),
        `${stem}-top.csv`, 'text/csv'),
    })
  }
  if (data?.kind === 'groups' && view === 'groups') {
    exportItems.push({
      label: 'Download CSV',
      run: () => downloadText(
        toCsv(['name', 'file', 'frames', 'value', 'count', 'mean', 'pct'],
          data.rows.map((r) => [r.name, r.file, r.frames.join(';'), r.value, r.count ?? '', r.mean ?? '', r.pct])),
        `${stem}-groups.csv`, 'text/csv'),
    })
  }
  exportItems.push(linkItem)

  const highlight = hot && data && (data.kind === 'flame' || data.kind === 'both') ? new Set(hotPath(data.tree)) : undefined

  return (
    <section aria-label="Profile">
      <div className="toolbar">
        <button onClick={() => navigate({}, 'push', true)}>&larr; Profiles</button>
        <strong className="mono">{record?.name ?? ref}</strong>
        {record && <FidelityBadge fidelity={record.fidelity} />}
        <div className="tabs" role="group" aria-label="View">
          {VIEW_LABELS.filter(([v]) => v !== 'time' || hasTime).map(([v, label]) => (
            <button key={v} aria-pressed={view === v} onClick={() => pushView({ view: v })}>{label}</button>
          ))}
        </div>
        {HOT_VIEWS.includes(view) && (
          <button aria-pressed={hot} onClick={toggleHot} title="Highlight the hottest call path">
            Hot path (h)
          </button>
        )}
        <ExportMenu items={exportItems} />
        <button onClick={() => setHelpOpen(true)} aria-label="Keyboard shortcuts" title="Keyboard shortcuts (?)">?</button>
      </div>
      <div className="toolbar">
        <label className="field">
          Sample type
          <select
            value={st ?? ''}
            disabled={!record}
            onChange={(e) => pushView({ st: Number(e.target.value) })}
          >
            {record?.sample_types.map((t, i) => (
              <option key={i} value={i}>{t.name} ({t.unit})</option>
            ))}
          </select>
        </label>
        <label className="field">
          Compare with…
          <select
            value=""
            disabled={others.length === 0}
            onChange={(e) => e.target.value && pushView({ view: 'diff', base: e.target.value, t0: undefined, t1: undefined, b0: undefined, b1: undefined })}
          >
            <option value="">{others.length === 0 ? 'no other profiles of this kind' : 'choose a profile'}</option>
            {others.map((p) => (
              <option key={p.id} value={p.short_id}>{p.name}</option>
            ))}
          </select>
        </label>
        <label className="field">Focus<input value={focus} onChange={(e) => setFocus(e.target.value)} placeholder="regex" /></label>
        <label className="field">Exclude<input value={exclude} onChange={(e) => setExclude(e.target.value)} placeholder="regex" /></label>
        <label className="field">Hide<input value={hide} onChange={(e) => setHide(e.target.value)} placeholder="regex" /></label>
        <label className="field">Search<input ref={searchRef} type="search" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="highlight" /></label>
      </div>
      {(t0 !== undefined || state.b0 !== undefined) && (
        <div className="toolbar" role="group" aria-label="Time selection">
          {t0 !== undefined && t1 !== undefined && (
            <span className="chip">
              <span className="mono">Window: {formatRange({ t0, t1 })}</span>
              <button aria-label="Clear window" onClick={() => navigate({ t0: undefined, t1: undefined }, 'push')}>×</button>
            </span>
          )}
          {state.b0 !== undefined && state.b1 !== undefined && (
            <span className="chip">
              <span className="mono">Baseline: {formatRange({ t0: state.b0, t1: state.b1 })}</span>
              <button aria-label="Clear baseline" onClick={() => navigate({ b0: undefined, b1: undefined }, 'push')}>×</button>
            </span>
          )}
          {t0 !== undefined && state.b0 !== undefined && (
            <button onClick={() => navigate({ view: 'diff', base: ref }, 'push')}>Diff window vs baseline</button>
          )}
        </div>
      )}
      <TransformBar tx={tx} onChange={(next) => navigate({ tx: next }, 'push')} />
      {exportMsg && <p className="muted" role="status">{exportMsg}</p>}
      {error && <div className="banner" role="alert">{error}</div>}
      {loading && !error && <p className="muted" role="status">Loading...</p>}
      {view === 'sandwich' && func === undefined && (
        <p className="muted">Pick a function in the Top view to see its callers and callees.</p>
      )}
      <div style={{ opacity: loading ? 0.5 : 1 }}>
      {view === 'summary' && record && (
        <Summary
          profileRef={ref}
          record={record}
          query={{ st, focus: qf, exclude: qe, hide: qh, depth, tx, t0, t1 }}
          navigate={navigate}
        />
      )}
      {data?.kind === 'flame' && (view === 'flame' || view === 'icicle') && (
        <FlameGraph
          root={data.tree}
          unit={unit}
          inverted={view === 'icicle'}
          search={state.search}
          highlight={highlight}
          exportRef={exportRef}
          onSelectFunc={(f) => pushView({ view: 'sandwich', func: f })}
        />
      )}
      {data?.kind === 'flame' && view === 'tree' && (
        <CallTree
          root={data.tree}
          unit={unit}
          hot={hot}
          onSelectFunc={(f) => pushView({ view: 'sandwich', func: f })}
          onTransform={onTransform}
        />
      )}
      {data?.kind === 'both' && view === 'both' && (
        <>
          <FlameGraph
            root={data.tree}
            unit={unit}
            inverted={false}
            search={state.search}
            highlight={highlight}
            exportRef={exportRef}
            onSelectFunc={(f) => pushView({ view: 'sandwich', func: f })}
          />
          <TopTable
            rows={data.rows}
            unit={unit}
            sort={sort}
            onSort={(s) => pushView({ sort: s === 'self' ? undefined : s })}
            onSelect={(f) => pushView({ view: 'sandwich', func: f })}
          />
        </>
      )}
      {data?.kind === 'top' && view === 'top' && (
        <TopTable
          rows={data.rows}
          unit={unit}
          sort={sort}
          onSort={(s) => pushView({ sort: s === 'self' ? undefined : s })} onSelect={(f) => pushView({ view: 'sandwich', func: f })} />
      )}
      {data?.kind === 'sandwich' && view === 'sandwich' && (
        <>
          <PivotPanel
            sessionId={record?.session_id ?? null}
            functionName={data.data.callees.name || null}
            currentProfileId={record?.id ?? ''}
            navigate={navigate}
          />
          <Sandwich data={data.data} unit={unit} search={state.search} onSelectFunc={(f) => pushView({ func: f })} />
        </>
      )}
      {data?.kind === 'groups' && view === 'groups' && (
        <Groups rows={data.rows} unit={unit} by={groupBy} onBy={(by) => pushView({ groupBy: by })} />
      )}
      {view === 'time' && record && !record.has_timeline && (
        <p className="muted">This profile has no time data. Go CPU and .NET CPU/wall captures include it by default; imported files do not.</p>
      )}
      {data?.kind === 'heat' && view === 'time' && (
        <HeatMap
          heat={data.heat}
          range={t0 !== undefined && t1 !== undefined ? { t0, t1 } : undefined}
          baseline={state.b0 !== undefined && state.b1 !== undefined ? { t0: state.b0, t1: state.b1 } : undefined}
          onRange={(r) => replace({ t0: r?.t0, t1: r?.t1 })}
          onBaseline={(r) => replace({ b0: r?.t0, b1: r?.t1 })}
        />
      )}
      {data?.kind === 'graph' && view === 'graph' && (
        <GraphView
          graph={data.graph}
          unit={unit}
          nodes={state.graphNodes ?? 80}
          onNodes={(n) => replace({ graphNodes: n })}
          onOpenFunc={(f) => pushView({ view: 'sandwich', func: f })}
          onTransform={onTransform}
        />
      )}
      </div>
      {helpOpen && (
        <div className="help-overlay" onClick={() => setHelpOpen(false)}>
          <div
            className="help-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Keyboard shortcuts"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 style={{ marginTop: 0 }}>Keyboard shortcuts</h2>
            <dl>
              {SHORTCUTS.map((k) => (
                <div key={k.keys} style={{ display: 'contents' }}>
                  <dt>{k.keys}</dt>
                  <dd>{k.label}</dd>
                </div>
              ))}
            </dl>
            <button autoFocus onClick={() => setHelpOpen(false)}>Close (Esc)</button>
          </div>
        </div>
      )}
    </section>
  )
}
