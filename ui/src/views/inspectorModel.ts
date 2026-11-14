import type { FlameNode } from '../api'

export type ChildShare = { name: string; funcId: number | null; total: number; pct: number }

/** The direct children of a node, heaviest first, with their share of the node's total. */
export function topChildren(node: FlameNode, n: number): ChildShare[] {
  return [...node.children]
    .sort((a, b) => b.total - a.total)
    .slice(0, Math.max(0, n))
    .map((c) => ({ name: c.name, funcId: c.funcId, total: c.total, pct: node.total > 0 ? (c.total / node.total) * 100 : 0 }))
}
