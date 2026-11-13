import type { DiffNode, FlameNode } from '../api'

/** A flame node that remembers the base and head values it was built from. */
export type DiffFlameNode = FlameNode & { base: number; head: number; baseSelf: number; headSelf: number }

/** Iterative conversion so deep diff trees cannot overflow the stack. */
export function toFlameNodes(root: DiffNode): DiffFlameNode {
  const make = (n: DiffNode): DiffFlameNode => ({
    funcId: null,
    name: n.name,
    file: n.file,
    selfValue: Math.max(n.baseSelf, n.headSelf),
    total: Math.max(n.baseTotal, n.headTotal),
    children: [],
    base: n.baseTotal,
    head: n.headTotal,
    baseSelf: n.baseSelf,
    headSelf: n.headSelf,
  })
  const out = make(root)
  const stack: [DiffNode, DiffFlameNode][] = [[root, out]]
  while (stack.length) {
    const [n, o] = stack.pop()!
    for (const c of n.children) {
      const co = make(c)
      o.children.push(co)
      stack.push([c, co])
    }
  }
  return out
}

