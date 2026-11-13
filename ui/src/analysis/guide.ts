import type { TopRow } from '../api'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'

export type GuideAction = { label: string; patch: Partial<ViewState> }
export type Guide = { title: string; questions: string[]; actions: GuideAction[]; measured: string }

const GRAPH: GuideAction = { label: 'Call graph', patch: { view: 'graph' } }
const FLAME: GuideAction = { label: 'Flame graph', patch: { view: 'flame' } }
const TREE: GuideAction = { label: 'Call tree', patch: { view: 'tree' } }
const TOP: GuideAction = { label: 'Top functions', patch: { view: 'top' } }
const HOT: GuideAction = { label: 'Hottest path', patch: { view: 'flame', hot: true } }
const WAITS: GuideAction = { label: 'Waits by site', patch: { view: 'groups', groupBy: 'site' } }
const TOP_TOTAL: GuideAction = { label: 'Top by total', patch: { view: 'top', sort: 'total' } }
const STACKS: GuideAction = { label: 'Identical stacks', patch: { view: 'groups', groupBy: 'stack' } }

const GUIDES: Record<string, Guide> = {
  cpu: {
    title: 'CPU profile',
    questions: [
      'Which function burns the most CPU?',
      'Which call path leads there?',
      'What changed since a baseline? Use Diff.',
    ],
    actions: [HOT, FLAME, TREE, GRAPH, TOP],
    measured: 'Sampled on-CPU time per stack by the runtime profiler.',
  },
  wall: {
    title: 'Wall-clock profile',
    questions: ['Where do threads wait?', 'Is the time spent running or sleeping on this path?'],
    actions: [HOT, FLAME, TREE, GRAPH, TOP],
    measured: 'Wall-clock samples of every thread or goroutine, running or waiting.',
  },
  memory: {
    title: 'Memory profile',
    questions: [
      'Who allocates the most?',
      'Switch the sample type (alloc space, alloc objects, in use) to change the question.',
      'What grew between two sessions? Use Diff.',
    ],
    actions: [FLAME, TOP, TREE],
    measured: 'Allocations and live heap per stack from the runtime heap profiler.',
  },
  lock: {
    title: 'Lock contention',
    questions: ['Which lock or call site has the longest total wait?', 'Who is waiting on it?'],
    actions: [WAITS, TOP_TOTAL, FLAME, TREE, GRAPH],
    measured: 'Time spent waiting on contended locks, per stack, from the runtime.',
  },
  blocking: {
    title: 'Blocking profile',
    questions: ['What are goroutines blocked on (channels, select, sync)?', 'Which call sites wait the longest?'],
    actions: [WAITS, TOP_TOTAL, FLAME, TREE, GRAPH],
    measured: 'Time spent blocked on channels, selects and sync primitives, per stack.',
  },
  block_io: {
    title: 'Block I/O',
    questions: ['Which files cost the most I/O time?', 'Which call paths wait on them?'],
    actions: [{ label: 'Slowest files', patch: { view: 'top' } }, TOP_TOTAL, FLAME, TREE, GRAPH],
    measured:
      'Time between an I/O request and its completion, per stack, from OS tracing (ETW on Windows, eBPF on Linux).',
  },
  off_cpu: {
    title: 'Off-CPU time',
    questions: ['What are threads waiting for when they are not running?', 'Which call paths spend the most time off the CPU?'],
    actions: [WAITS, TOP_TOTAL, FLAME, TREE, GRAPH],
    measured: 'Time threads spend descheduled, per stack, from OS tracing of context switches.',
  },
  sched_delay: {
    title: 'Scheduler delay',
    questions: ['Are runnable threads waiting for a CPU?', 'Is the container being throttled?'],
    actions: [TOP_TOTAL, FLAME, TREE],
    measured: 'Time between a thread becoming runnable and getting a CPU, from OS scheduler events.',
  },
  threads: {
    title: 'Thread / goroutine dump',
    questions: ['Are many threads parked in the same place?', 'Which stacks look stuck?'],
    actions: [STACKS, TREE, FLAME, TOP],
    measured: 'A snapshot of every thread or goroutine and its stack at one moment.',
  },
}

const GENERIC: Guide = {
  title: 'Profile',
  questions: ['Which function holds the most self time?', 'Which call path leads there?'],
  actions: [FLAME, TREE, GRAPH, TOP],
  measured: 'Measured by the capture tool that produced this profile.',
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
