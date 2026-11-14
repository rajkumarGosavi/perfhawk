import type { ViewName } from '../url'

export type TermKey =
  | 'self' | 'total' | 'sampleType' | 'fidelity' | 'session' | 'profile' | 'window' | 'baseline'
  | 'hotPath' | 'focus' | 'exclude' | 'hide' | 'flame' | 'icicle' | 'sandwich'

export type Term = { label: string; short: string; long?: string }

export const GLOSSARY: Record<TermKey, Term> = {
  self: {
    label: 'Self',
    short: 'Time spent in this function itself, not in functions it calls. Total includes callees.',
    long: 'A function with high self time is doing the work itself. If self is small but total is large, the cost is in what it calls.',
  },
  total: {
    label: 'Total',
    short: 'Everything under this function: its own work (self) plus everything it calls.',
    long: 'A recursive function is counted once per sample in Total, so a function can never exceed 100%.',
  },
  sampleType: {
    label: 'Sample type',
    short: 'Which measurement to analyse, for example CPU time, or bytes allocated vs bytes still in use.',
  },
  fidelity: {
    label: 'Fidelity',
    short: 'How the numbers were obtained: native (measured by the runtime), approximate, derived, sampled external, or unknown (imported file).',
  },
  session: {
    label: 'Session',
    short: 'One recording run (`perfhawk record`). It groups the profiles captured together, such as CPU and memory for the same run.',
  },
  profile: {
    label: 'Profile',
    short: 'One set of measurements, from a recording or an imported file. Open one to see where time or memory goes.',
  },
  window: {
    label: 'Window',
    short: "A slice of the recording's timeline. The graphs then show only what happened inside it.",
  },
  baseline: {
    label: 'Baseline',
    short: 'A reference slice (or earlier profile) to compare against, to see what changed.',
  },
  hotPath: {
    label: 'Hot path',
    short: 'The call path from the root that, at every step, follows the child with the biggest total. It is the quickest route to the expensive code.',
  },
  focus: {
    label: 'Focus',
    short: 'Keep only call paths that match this regular expression, so everything else disappears.',
  },
  exclude: {
    label: 'Exclude',
    short: 'Drop samples whose stack matches this regular expression.',
  },
  hide: {
    label: 'Hide',
    short: 'Hide matching frames but keep their cost, merged into their callers.',
  },
  flame: {
    label: 'Flame graph',
    short: 'Each box is a function; its width is its total cost. The root is at the bottom, callees stack on top.',
  },
  icicle: {
    label: 'Icicle graph',
    short: 'The same as a flame graph, upside down: the root is at the top.',
  },
  sandwich: {
    label: 'Sandwich view',
    short: 'One function with its callers above and its callees below: who calls it, and where it spends its time.',
  },
}

export type HelpTopic = { id: string; title: string; body: string[] }

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: 'getting-started',
    title: 'Getting started',
    body: [
      'Perfhawk shows where your application spends CPU, memory and waiting time, so you can find what to fix.',
      'A session is one recording run. It holds one or more profiles, for example CPU and memory for the same run. A profile is one set of measurements; open it to see the graphs.',
      'Record a running program with `perfhawk record <target> --open`, or import a profile file with the Import box on the Profiles tab.',
      'Open a profile and start on the Summary tab: it names the most expensive function and suggests where to look next.',
    ],
  },
  {
    id: 'flame-graph',
    title: 'Reading a flame graph',
    body: [
      'Each box is a function in a call path. A wider box means more of the total cost. Callees sit on top of their caller (the icicle view is the same graph upside down).',
      'Hover or focus a box to see its name, self and total. Click it (or press Enter) to zoom in; Esc resets the zoom.',
      'Look for wide boxes with little on top of them: that is where the time is really spent.',
    ],
  },
  {
    id: 'self-total',
    title: 'Self vs Total',
    body: [
      'Self is the cost of the function itself. Total is self plus everything it calls.',
      'Sort by Self to find the functions doing the work. Sort by Total to find the entry points that lead to expensive work.',
    ],
  },
  {
    id: 'compare',
    title: 'Comparing runs',
    body: [
      'Use Compare with... on a profile to diff it against another of the same kind. Red frames grew, green frames shrank, and grey ones did not change.',
      'Diff a window against a baseline slice of one recording with the Time view, or follow one function over many sessions in History.',
    ],
  },
  {
    id: 'shortcuts',
    title: 'Keyboard shortcuts',
    body: [],
  },
  {
    id: 'troubleshooting',
    title: 'Troubleshooting',
    body: [
      "If you see \"Can't reach the Perfhawk server\", the server has stopped: run `perfhawk ui` again and reload.",
      'If a profile has no time data, it was imported from a file; recordings made by `perfhawk record` include a timeline.',
      'Run `perfhawk doctor` to check what Perfhawk could and could not capture.',
    ],
  },
]

const VIEW_TOPIC: Record<ViewName, string> = {
  summary: 'getting-started',
  flame: 'flame-graph',
  icicle: 'flame-graph',
  tree: 'self-total',
  top: 'self-total',
  both: 'flame-graph',
  sandwich: 'self-total',
  diff: 'compare',
  groups: 'self-total',
  graph: 'flame-graph',
  time: 'compare',
  history: 'compare',
}

export function topicForView(view: ViewName): string {
  return VIEW_TOPIC[view]
}

const TERM_TOPIC: Record<TermKey, string> = {
  self: 'self-total',
  total: 'self-total',
  sampleType: 'getting-started',
  fidelity: 'getting-started',
  session: 'getting-started',
  profile: 'getting-started',
  window: 'compare',
  baseline: 'compare',
  hotPath: 'flame-graph',
  focus: 'flame-graph',
  exclude: 'flame-graph',
  hide: 'flame-graph',
  flame: 'flame-graph',
  icicle: 'flame-graph',
  sandwich: 'self-total',
}

export function topicForTerm(k: TermKey): string {
  return TERM_TOPIC[k]
}
