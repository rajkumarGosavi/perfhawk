import type { FlameNode } from '../api'
import { formatPct, formatValue } from '../format'

export type NavDir = 'parent' | 'child' | 'prev' | 'next'

/** Parent of every node under `root` (iterative, so very deep trees cannot overflow the stack). */
function parents(root: FlameNode): Map<FlameNode, FlameNode> {
  const map = new Map<FlameNode, FlameNode>()
  const stack = [root]
  while (stack.length) {
    const p = stack.pop()!
    for (const c of p.children) {
      map.set(c, p)
      stack.push(c)
    }
  }
  return map
}

/**
 * The frame to move to from `from`, within the tree under `root` (the zoomed top, when zoomed).
 * Returns `from` when the move is impossible. `visible`, when given, limits movement to drawn frames
 * (frames too narrow to draw are not in the layout and must not be reachable).
 */
export function navigate(root: FlameNode, from: FlameNode, dir: NavDir, visible?: ReadonlySet<FlameNode>): FlameNode {
  const ok = (n: FlameNode) => !visible || visible.has(n)
  const up = parents(root)
  if (from !== root && !up.has(from)) return from
  switch (dir) {
    case 'parent':
      return from === root ? from : (up.get(from) ?? from)
    case 'child': {
      let best: FlameNode | undefined
      for (const c of from.children) if (ok(c) && (!best || c.total > best.total)) best = c
      return best ?? from
    }
    case 'prev':
    case 'next': {
      const p = up.get(from)
      if (!p) return from
      const sibs = p.children.filter(ok)
      const i = sibs.indexOf(from)
      if (i < 0) return from
      return sibs[dir === 'next' ? Math.min(i + 1, sibs.length - 1) : Math.max(i - 1, 0)]
    }
  }
}

/** One spoken sentence for the live region: name, self, total and share of the whole. */
export function describeFrame(node: FlameNode, root: FlameNode, unit: string): string {
  const share = root.total > 0 ? (node.total / root.total) * 100 : 0
  return `${node.name}, self ${formatValue(node.selfValue, unit)}, total ${formatValue(node.total, unit)} (${formatPct(share)} of total)`
}
