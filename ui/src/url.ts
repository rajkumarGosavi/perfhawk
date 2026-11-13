export type ViewName = 'summary' | 'flame' | 'icicle' | 'tree' | 'top' | 'both' | 'sandwich' | 'diff'
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
}

const VIEWS: ViewName[] = ['summary', 'flame', 'icicle', 'tree', 'top', 'both', 'sandwich', 'diff']

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
