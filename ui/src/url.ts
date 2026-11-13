export type ViewName = 'summary' | 'flame' | 'icicle' | 'tree' | 'top' | 'both' | 'sandwich' | 'diff' | 'groups' | 'graph' | 'time'
export type ViewState = {
  profile?: string
  /** Session to expand on the home page. */
  session?: string
  /** Diff base profile; `profile` is the head. */
  base?: string
  /** Diff normalisation; omitted when none. */
  normalize?: 'total'
  /** Flame depth cap; omitted for the server default. */
  depth?: number
  view: ViewName
  st?: number
  focus?: string
  exclude?: string
  hide?: string
  func?: number
  search?: string
  /** Top table ordering; 'self' is the default and is omitted from the URL. */
  sort?: 'self' | 'total'
  /** Highlight / expand the hottest call path; written as hot=1. */
  hot?: true
  /** Stack transforms (`tx`), see analysis/transforms.ts. */
  tx?: string
  /** Groups view grouping; written as gb. */
  groupBy?: 'stack' | 'site'
  /** Diff side-by-side layout; written as layout=side. */
  diffLayout?: 'side'
  /** Diff row filter; written as only. */
  diffFilter?: 'regressions' | 'improvements'
  /** Diff top ordering; written as dsort=percent, delta is the default. */
  diffSort?: 'percent'
  /** Call graph node budget (5..300); written as gn. */
  graphNodes?: number
  /** Time window in ms from capture start (both or neither). */
  t0?: number
  t1?: number
  /** Baseline window for a window-vs-window diff. */
  b0?: number
  b1?: number
}

const VIEWS: ViewName[] = ['summary', 'flame', 'icicle', 'tree', 'top', 'both', 'sandwich', 'diff', 'groups', 'graph', 'time']

function int(s: string | null): number | undefined {
  if (s === null || s === '') return undefined
  const n = Number(s)
  return Number.isInteger(n) && n >= 0 ? n : undefined
}

export function readViewState(search: string): ViewState {
  const p = new URLSearchParams(search)
  const v = p.get('view')
  const s: ViewState = { view: VIEWS.includes(v as ViewName) ? (v as ViewName) : 'summary' }
  const profile = p.get('profile')
  if (profile) s.profile = profile
  for (const k of ['session', 'base'] as const) {
    const x = p.get(k)
    if (x) s[k] = x
  }
  if (p.get('normalize') === 'total') s.normalize = 'total'
  const depth = int(p.get('depth'))
  if (depth !== undefined && depth >= 1) s.depth = depth
  const st = int(p.get('st'))
  if (st !== undefined) s.st = st
  for (const k of ['focus', 'exclude', 'hide', 'search'] as const) {
    const x = p.get(k)
    if (x) s[k] = x
  }
  const func = int(p.get('func'))
  if (func !== undefined) s.func = func
  if (p.get('sort') === 'total') s.sort = 'total'
  if (p.get('hot') === '1') s.hot = true
  const tx = p.get('tx')
  if (tx) s.tx = tx
  const gb = p.get('gb')
  if (gb === 'stack' || gb === 'site') s.groupBy = gb
  if (p.get('layout') === 'side') s.diffLayout = 'side'
  const only = p.get('only')
  if (only === 'regressions' || only === 'improvements') s.diffFilter = only
  if (p.get('dsort') === 'percent') s.diffSort = 'percent'
  const pair = (a: string, b: string): [number, number] | undefined => {
    const x = int(p.get(a))
    const y = int(p.get(b))
    return x !== undefined && y !== undefined && y > x ? [x, y] : undefined
  }
  const win = pair('t0', 't1')
  if (win) [s.t0, s.t1] = win
  const base = pair('b0', 'b1')
  if (base) [s.b0, s.b1] = base
  const gn = int(p.get('gn'))
  if (gn !== undefined && gn >= 5 && gn <= 300) s.graphNodes = gn
  return s
}

export function writeViewState(s: ViewState): string {
  const p = new URLSearchParams()
  if (s.profile) p.set('profile', s.profile)
  if (s.session) p.set('session', s.session)
  if (s.base) p.set('base', s.base)
  if (s.view !== 'summary') p.set('view', s.view)
  if (s.normalize === 'total') p.set('normalize', 'total')
  if (s.depth !== undefined) p.set('depth', String(s.depth))
  if (s.st !== undefined) p.set('st', String(s.st))
  if (s.focus) p.set('focus', s.focus)
  if (s.exclude) p.set('exclude', s.exclude)
  if (s.hide) p.set('hide', s.hide)
  if (s.func !== undefined) p.set('func', String(s.func))
  if (s.search) p.set('search', s.search)
  if (s.sort === 'total') p.set('sort', 'total')
  if (s.hot) p.set('hot', '1')
  if (s.tx) p.set('tx', s.tx)
  if (s.groupBy) p.set('gb', s.groupBy)
  if (s.diffLayout) p.set('layout', s.diffLayout)
  if (s.diffFilter) p.set('only', s.diffFilter)
  if (s.diffSort) p.set('dsort', s.diffSort)
  if (s.graphNodes !== undefined) p.set('gn', String(s.graphNodes))
  if (s.t0 !== undefined && s.t1 !== undefined) {
    p.set('t0', String(s.t0))
    p.set('t1', String(s.t1))
  }
  if (s.b0 !== undefined && s.b1 !== undefined) {
    p.set('b0', String(s.b0))
    p.set('b1', String(s.b1))
  }
  const q = p.toString()
  return q ? `?${q}` : ''
}

/** Merges a patch into the current state; keys patched to undefined are removed. */
export function mergeViewState(current: ViewState, patch: Partial<ViewState>): ViewState {
  const next: ViewState = { ...current, ...patch }
  for (const k of Object.keys(next) as (keyof ViewState)[]) {
    if (next[k] === undefined) delete next[k]
  }
  return next
}
