import { useEffect, useRef, useState } from 'react'
import {
  ApiError, getFlame, getProfile, getSandwich, getTop, listProfiles,
  type FlameNode, type ProfileRecord, type Sandwich as SandwichData, type TopRow,
} from '../api'
import { hotPath } from '../analysis/hotPath'
import { SHORTCUTS, keyAction } from '../analysis/keys'
import { FlameGraph } from '../flame/FlameGraph'
import type { ViewName, ViewState } from '../url'
import { CallTree } from './CallTree'
import { Sandwich } from './Sandwich'
import { Summary } from './Summary'
import { FidelityBadge } from './FidelityBadge'
import { TopTable } from './TopTable'

type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void
type Data =
  | { kind: 'flame'; tree: FlameNode }
  | { kind: 'top'; rows: TopRow[] }
  | { kind: 'both'; tree: FlameNode; rows: TopRow[] }
  | { kind: 'sandwich'; data: SandwichData }

const VIEW_LABELS: [ViewName, string][] = [
  ['summary', 'Summary'], ['flame', 'Flame'], ['icicle', 'Icicle'], ['tree', 'Call tree'],
  ['top', 'Top'], ['both', 'Both'], ['sandwich', 'Sandwich'],
]
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
  const { view, func, focus: qf, exclude: qe, hide: qh, depth } = state
  const sort = state.sort ?? 'self'

  useEffect(() => {
    if (!record) return
    if (view === 'summary' || (view === 'sandwich' && func === undefined)) {
      setData(null)
      setLoading(false)
      return
    }
    let stale = false
    setLoading(true)
    const q = { st, focus: qf, exclude: qe, hide: qh, depth }
    const p: Promise<Data> =
      view === 'top'
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
  }, [record, ref, view, func, st, qf, qe, qh, sort, depth])

  const pushView = (patch: Partial<ViewState>) => navigate(patch, 'push')
  const hot = !!state.hot
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

  const highlight = hot && data && (data.kind === 'flame' || data.kind === 'both') ? new Set(hotPath(data.tree)) : undefined

  return (
    <section aria-label="Profile">
      <div className="toolbar">
        <button onClick={() => navigate({}, 'push', true)}>&larr; Profiles</button>
        <strong className="mono">{record?.name ?? ref}</strong>
        {record && <FidelityBadge fidelity={record.fidelity} />}
        <div className="tabs" role="group" aria-label="View">
          {VIEW_LABELS.map(([v, label]) => (
            <button key={v} aria-pressed={view === v} onClick={() => pushView({ view: v })}>{label}</button>
          ))}
        </div>
        {HOT_VIEWS.includes(view) && (
          <button aria-pressed={hot} onClick={toggleHot} title="Highlight the hottest call path">
            Hot path (h)
          </button>
        )}
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
            onChange={(e) => e.target.value && pushView({ view: 'diff', base: e.target.value })}
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
          query={{ st, focus: qf, exclude: qe, hide: qh, depth }}
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
          onSelectFunc={(f) => pushView({ view: 'sandwich', func: f })}
        />
      )}
      {data?.kind === 'flame' && view === 'tree' && (
        <CallTree
          root={data.tree}
          unit={unit}
          hot={hot}
          onSelectFunc={(f) => pushView({ view: 'sandwich', func: f })}
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
        <Sandwich data={data.data} unit={unit} search={state.search} onSelectFunc={(f) => pushView({ func: f })} />
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
