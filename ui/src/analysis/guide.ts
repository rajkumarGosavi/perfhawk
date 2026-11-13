import type { TopRow } from '../api'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'

export type GuideAction = { label: string; patch: Partial<ViewState> }
export type Guide = { title: string; questions: string[]; actions: GuideAction[] }

const FLAME: GuideAction = { label: 'Flame graph', patch: { view: 'flame' } }
const TREE: GuideAction = { label: 'Call tree', patch: { view: 'tree' } }
const TOP: GuideAction = { label: 'Top functions', patch: { view: 'top' } }
const HOT: GuideAction = { label: 'Hottest path', patch: { view: 'flame', hot: true } }
const WAITS: GuideAction = { label: 'Longest waits (by total)', patch: { view: 'top', sort: 'total' } }

const GUIDES: Record<string, Guide> = {
  cpu: {
    title: 'CPU profile',
    questions: [
      'Which function burns the most CPU?',
      'Which call path leads there?',
      'What changed since a baseline? Use Diff.',
    ],
    actions: [HOT, FLAME, TREE, TOP],
  },
  wall: {
    title: 'Wall-clock profile',
    questions: ['Where do threads wait?', 'Is the time spent running or sleeping on this path?'],
    actions: [HOT, FLAME, TREE, TOP],
  },
  memory: {
    title: 'Memory profile',
    questions: [
      'Who allocates the most?',
      'Switch the sample type (alloc space, alloc objects, in use) to change the question.',
      'What grew between two sessions? Use Diff.',
    ],
    actions: [FLAME, TOP, TREE],
  },
  lock: {
    title: 'Lock contention',
    questions: ['Which lock or call site has the longest total wait?', 'Who is waiting on it?'],
    actions: [WAITS, FLAME, TREE],
  },
  blocking: {
    title: 'Blocking profile',
    questions: ['What are goroutines blocked on (channels, select, sync)?', 'Which call sites wait the longest?'],
    actions: [WAITS, FLAME, TREE],
  },
  threads: {
    title: 'Thread / goroutine dump',
    questions: ['Are many threads parked in the same place?', 'Which stacks look stuck?'],
    actions: [TREE, FLAME, TOP],
  },
}

const GENERIC: Guide = {
  title: 'Profile',
  questions: ['Which function holds the most self time?', 'Which call path leads there?'],
  actions: [FLAME, TREE, TOP],
}

export function kindGuide(kind: string): Guide {
  return Object.hasOwn(GUIDES, kind) ? GUIDES[kind] : GENERIC
}

const EMPTY = 'No samples in this view.'

/** One sentence about the largest self-time function; never emits NaN. */
export function headline(rows: TopRow[], unit: string): string {
  const r = rows[0]
  if (!r || !Number.isFinite(r.selfPct) || r.selfPct <= 0) return EMPTY
  return `${r.name} holds ${formatPct(r.selfPct)} of self ${formatValue(r.selfValue, unit)}`
}

export function concentration(rows: TopRow[]): 'concentrated' | 'spread' {
  return rows.length > 0 && rows[0].selfPct >= 30 ? 'concentrated' : 'spread'
}
