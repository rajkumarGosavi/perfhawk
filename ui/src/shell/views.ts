import type { ViewName } from '../url'

export type RailItem = {
  view: ViewName
  label: string
  /** Single-key shortcut. */
  key: string
  group: 'analyze' | 'compare'
  needsTimeline?: boolean
  /** Only when the profile's session has GC data. */
  needsGc?: boolean
  /** Only when another profile of the same kind exists to compare with. */
  needsDiff?: boolean
}

/** The views on the left rail, in key order. */
export const RAIL: RailItem[] = [
  { view: 'summary', label: 'Summary', key: '1', group: 'analyze' },
  { view: 'flame', label: 'Flame graph', key: '2', group: 'analyze' },
  { view: 'tree', label: 'Call tree', key: '3', group: 'analyze' },
  { view: 'time', label: 'Timeline', key: '4', group: 'analyze', needsTimeline: true },
  { view: 'graph', label: 'Call graph', key: '5', group: 'analyze' },
  { view: 'gc', label: 'Garbage collection', key: '6', group: 'analyze', needsGc: true },
  { view: 'diff', label: 'Diff', key: '7', group: 'compare', needsDiff: true },
  { view: 'history', label: 'History', key: '8', group: 'compare' },
]

/** The rail item a view lights up: legacy flame variants (and the standalone Top and Groups tables) map to Flame graph. */
export function railView(view: ViewName): ViewName {
  return view === 'icicle' || view === 'both' || view === 'sandwich' || view === 'top' || view === 'groups' ? 'flame' : view
}
