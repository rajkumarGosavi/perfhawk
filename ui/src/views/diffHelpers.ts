import type { DiffNode, DiffRow } from '../api'
import type { DiffFlameNode } from './diffTree'

export type DiffStatus = 'new' | 'gone' | 'changed' | 'same'
export type DiffFilterMode = 'regressions' | 'improvements' | undefined

export function diffStatus(base: number, head: number): DiffStatus {
  if (base === head) return 'same'
  if (base === 0) return 'new'
  if (head === 0) return 'gone'
  return 'changed'
}

const matches = (delta: number, mode: 'regressions' | 'improvements') =>
  mode === 'regressions' ? delta > 0 : delta < 0

export function filterRows(rows: DiffRow[], mode: DiffFilterMode): DiffRow[] {
  if (!mode) return rows
  return rows.filter((r) => matches(r.deltaSelf, mode))
}

/** Keeps nodes whose total delta has the mode's sign; the root always stays. Iterative. */
export function filterDiffTree(root: DiffNode, mode: DiffFilterMode): DiffNode {
  if (!mode) return root
  const out: DiffNode = { ...root, children: [] }
  const stack: [DiffNode, DiffNode][] = [[root, out]]
  while (stack.length) {
    const [src, dst] = stack.pop()!
    for (const c of src.children) {
      if (!matches(c.headTotal - c.baseTotal, mode)) continue
      const copy: DiffNode = { ...c, children: [] }
      dst.children.push(copy)
      stack.push([c, copy])
    }
  }
  return out
}

/** One side of a diff as a flame tree; nodes with no weight on that side are omitted. Iterative. */
export function toSideTree(root: DiffNode, side: 'base' | 'head'): DiffFlameNode {
  const make = (n: DiffNode): DiffFlameNode => ({
    funcId: null,
    name: n.name,
    file: n.file,
    selfValue: side === 'base' ? n.baseSelf : n.headSelf,
    total: side === 'base' ? n.baseTotal : n.headTotal,
    children: [],
    base: n.baseTotal,
    head: n.headTotal,
    baseSelf: n.baseSelf,
    headSelf: n.headSelf,
  })
  const out = make(root)
  const stack: [DiffNode, DiffFlameNode][] = [[root, out]]
  while (stack.length) {
    const [src, dst] = stack.pop()!
    for (const c of src.children) {
      const co = make(c)
      if (co.total === 0) continue
      dst.children.push(co)
      stack.push([c, co])
    }
  }
  return out
}

/** A glyph for the direction of a change, so growth and shrinkage never rely on colour alone. */
export function directionMark(delta: number): '▲' | '▼' | '' {
  return delta > 0 ? '▲' : delta < 0 ? '▼' : ''
}
