export type ViewName = 'flame' | 'icicle' | 'top' | 'sandwich'
export type ViewState = {
  profile?: string
  view: ViewName
  st?: number
  focus?: string
  exclude?: string
  hide?: string
  func?: number
  search?: string
}

const VIEWS: ViewName[] = ['flame', 'icicle', 'top', 'sandwich']

function int(s: string | null): number | undefined {
  if (s === null || s === '') return undefined
  const n = Number(s)
  return Number.isInteger(n) && n >= 0 ? n : undefined
}

export function readViewState(search: string): ViewState {
  const p = new URLSearchParams(search)
  const v = p.get('view')
  const s: ViewState = { view: VIEWS.includes(v as ViewName) ? (v as ViewName) : 'flame' }
  const profile = p.get('profile')
  if (profile) s.profile = profile
  const st = int(p.get('st'))
  if (st !== undefined) s.st = st
  for (const k of ['focus', 'exclude', 'hide', 'search'] as const) {
    const x = p.get(k)
    if (x) s[k] = x
  }
  const func = int(p.get('func'))
  if (func !== undefined) s.func = func
  return s
}

export function writeViewState(s: ViewState): string {
  const p = new URLSearchParams()
  if (s.profile) p.set('profile', s.profile)
  if (s.view !== 'flame') p.set('view', s.view)
  if (s.st !== undefined) p.set('st', String(s.st))
  if (s.focus) p.set('focus', s.focus)
  if (s.exclude) p.set('exclude', s.exclude)
  if (s.hide) p.set('hide', s.hide)
  if (s.func !== undefined) p.set('func', String(s.func))
  if (s.search) p.set('search', s.search)
  const q = p.toString()
  return q ? `?${q}` : ''
}
