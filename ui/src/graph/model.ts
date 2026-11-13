import type { CallGraph, GraphNode } from '../api'
import type { Placed } from './layout'

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** Self fraction of the profile, 0..1; 0 when the total is 0. */
export function heat(n: GraphNode, totalValue: number): number {
  if (!(totalValue > 0)) return 0
  return clamp(n.selfValue / totalValue, 0, 1)
}

/** Box size: width from the name length, height from how much self cost the node holds. */
export function nodeSize(n: GraphNode, totalValue: number): { w: number; h: number } {
  return { w: clamp(n.name.length * 7 + 24, 80, 260), h: Math.round(24 + 40 * heat(n, totalValue)) }
}

export function edgeWidth(value: number, maxValue: number): number {
  if (!(maxValue > 0) || !(value > 0)) return 1
  return clamp(1 + (7 * value) / maxValue, 1, 8)
}

/** Callers and callees of a node, heaviest edge first. */
export function neighbours(g: CallGraph, id: number): { callers: number[]; callees: number[] } {
  const byValue = (a: { value: number }, b: { value: number }) => b.value - a.value
  return {
    callers: g.edges.filter((e) => e.to === id).sort(byValue).map((e) => e.from),
    callees: g.edges.filter((e) => e.from === id).sort(byValue).map((e) => e.to),
  }
}

export type Dir = 'up' | 'down' | 'left' | 'right'

/** Next selected node for an arrow key; stays on the current node when there is no neighbour. */
export function moveFocus(g: CallGraph, placed: Placed[], current: number | null, dir: Dir): number | null {
  if (g.nodes.length === 0) return null
  if (current === null || !g.nodes.some((n) => n.funcId === current)) {
    return g.nodes.reduce((best, n) => (n.total > best.total ? n : best)).funcId
  }
  if (dir === 'up') return neighbours(g, current).callers[0] ?? current
  if (dir === 'down') return neighbours(g, current).callees[0] ?? current
  const me = placed.find((p) => p.id === current)
  if (!me) return current
  const row = placed.filter((p) => p.layer === me.layer).sort((a, b) => a.x - b.x)
  const i = row.findIndex((p) => p.id === current)
  const next = row[dir === 'left' ? i - 1 : i + 1]
  return next ? next.id : current
}
