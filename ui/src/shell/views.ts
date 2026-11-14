import type { ViewName } from '../url'

export type RailItem = {
  view: ViewName
  label: string
  /** Single-key shortcut. */
  key: string
  group: 'analyze' | 'compare'
  needsTimeline?: boolean
}

/** The views on the left rail, in key order. */
export const RAIL: RailItem[] = [
  { view: 'summary', label: 'Summary', key: '1', group: 'analyze' },
  { view: 'flame', label: 'Flame graph', key: '2', group: 'analyze' },
  { view: 'tree', label: 'Call tree', key: '3', group: 'analyze' },
  { view: 'top', label: 'Top functions', key: '4', group: 'analyze' },
  { view: 'time', label: 'Timeline', key: '5', group: 'analyze', needsTimeline: true },
  { view: 'graph', label: 'Call graph', key: '6', group: 'analyze' },
  { view: 'groups', label: 'Groups', key: '7', group: 'analyze' },
  { view: 'history', label: 'History', key: '8', group: 'compare' },
]

/** The rail item a view lights up: legacy flame variants map to Flame graph. */
export function railView(view: ViewName): ViewName {
  return view === 'icicle' || view === 'both' || view === 'sandwich' ? 'flame' : view
}
