import type { Finding, FindingAction } from '../api'
import type { ViewName, ViewState } from '../url'

const VIEWS: ViewName[] = ['summary', 'flame', 'icicle', 'tree', 'top', 'both', 'sandwich', 'diff', 'groups']
const STORAGE_KEY = 'perfhawk.findings.disabled'

/** Turns a finding's action into a URL-state patch; omitted fields stay absent. */
export function actionToPatch(a: FindingAction): Partial<ViewState> {
  const patch: Partial<ViewState> = { view: VIEWS.includes(a.view as ViewName) ? (a.view as ViewName) : 'flame' }
  if (a.funcId !== null) patch.func = a.funcId
  if (a.focus) patch.focus = a.focus
  if (a.tx) patch.tx = a.tx
  if (a.sort === 'self' || a.sort === 'total') patch.sort = a.sort
  if (a.groupBy === 'stack' || a.groupBy === 'site') patch.groupBy = a.groupBy
  return patch
}

export function loadDisabled(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return new Set()
    const v: unknown = JSON.parse(raw)
    return Array.isArray(v) ? new Set(v.filter((x): x is string => typeof x === 'string')) : new Set()
  } catch {
    return new Set()
  }
}

export function saveDisabled(s: Set<string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...s]))
  } catch {
    /* storage unavailable: the choice lasts for this page only */
  }
}

export function visibleFindings(all: Finding[], disabled: ReadonlySet<string>): Finding[] {
  return all.filter((f) => !disabled.has(f.rule))
}
