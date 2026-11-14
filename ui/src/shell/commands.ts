import type { Nav } from '../nav'
import type { Density, ThemePref } from '../theme'
import { RAIL } from './views'

export type Command = { id: string; label: string; group: 'Views' | 'App'; hint?: string; run: () => void }

export type CommandCtx = {
  hasProfile: boolean
  hasTimeline: boolean
  hasGc?: boolean
  /** Short id of a profile to compare with, when one exists. */
  diffBase?: string
  /** Current compare base, kept when going to Diff again. */
  base?: string
  navigate: Nav
  openHelp: () => void
  setTheme: (p: ThemePref) => void
  setDensity?: (d: Density) => void
  focusSearch: () => void
}

const THEMES: [ThemePref, string][] = [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']]

/** The commands the palette lists for the current page. */
export function buildCommands(ctx: CommandCtx): Command[] {
  const out: Command[] = []
  for (const r of RAIL) {
    if (r.view === 'history') {
      out.push({ id: 'go-history', label: `Go to ${r.label}`, group: 'Views', hint: r.key, run: () => ctx.navigate({ view: 'history' }, 'push', true) })
    } else if (ctx.hasProfile && (!r.needsTimeline || ctx.hasTimeline) && (!r.needsGc || ctx.hasGc) && (!r.needsDiff || ctx.diffBase)) {
      out.push({ id: `go-${r.view}`, label: `Go to ${r.label}`, group: 'Views', hint: r.key, run: () => ctx.navigate(r.view === 'diff' ? { view: 'diff', base: ctx.base ?? ctx.diffBase } : { view: r.view }, 'push') })
    }
  }
  out.push({ id: 'go-home', label: 'Go to home', group: 'Views', run: () => ctx.navigate({}, 'push', true) })
  if (ctx.hasProfile) {
    out.push({ id: 'go-top', label: 'Go to Top functions table', group: 'Views', run: () => ctx.navigate({ view: 'top' }, 'push') })
    out.push({ id: 'go-groups', label: 'Go to Groups table', group: 'Views', run: () => ctx.navigate({ view: 'groups' }, 'push') })
  }
  if (ctx.hasProfile) out.push({ id: 'focus-search', label: 'Focus search', group: 'App', hint: '/', run: ctx.focusSearch })
  out.push({ id: 'help', label: 'Open help', group: 'App', hint: '?', run: ctx.openHelp })
  for (const [value, label] of THEMES) out.push({ id: `theme-${value}`, label: `Theme: ${label}`, group: 'App', run: () => ctx.setTheme(value) })
  const setDensity = ctx.setDensity
  if (setDensity) {
    out.push({ id: 'density-comfortable', label: 'Density: Comfortable', group: 'App', run: () => setDensity('comfortable') })
    out.push({ id: 'density-compact', label: 'Density: Compact', group: 'App', run: () => setDensity('compact') })
  }
  return out
}
