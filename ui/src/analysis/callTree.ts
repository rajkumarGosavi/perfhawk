import type { FlameNode } from '../api'

export type TreeRow =
  | { kind: 'node'; key: string; node: FlameNode; depth: number; hasChildren: boolean; expanded: boolean }
  | { kind: 'more'; key: string; depth: number; count: number }

const childKey = (key: string, i: number) => (key === '' ? String(i) : `${key}/${i}`)

/** Pre-order visible rows. Iterative: trees can be 10 000 deep. */
export function visibleRows(root: FlameNode, expanded: ReadonlySet<string>, maxChildren = 200): TreeRow[] {
  const rows: TreeRow[] = []
  type Item = { row: TreeRow; node?: FlameNode }
  const stack: Item[] = [
    { row: { kind: 'node', key: '', node: root, depth: 0, hasChildren: false, expanded: false }, node: root },
  ]
  while (stack.length > 0) {
    const { row, node } = stack.pop() as Item
    if (row.kind === 'more' || !node) {
      rows.push(row)
      continue
    }
    const isOpen = expanded.has(row.key) && node.children.length > 0
    rows.push({ ...row, hasChildren: node.children.length > 0, expanded: isOpen })
    if (!isOpen) continue
    const shown = Math.min(node.children.length, maxChildren)
    if (node.children.length > shown) {
      stack.push({
        row: { kind: 'more', key: row.key + '/…', depth: row.depth + 1, count: node.children.length - shown },
      })
    }
    for (let i = shown - 1; i >= 0; i--) {
      const c = node.children[i]
      stack.push({
        row: { kind: 'node', key: childKey(row.key, i), node: c, depth: row.depth + 1, hasChildren: false, expanded: false },
        node: c,
      })
    }
  }
  return rows
}

/** Keys to expand so the hot path is visible, plus the key of its last node. */
export function hotPathKeys(root: FlameNode): { expand: Set<string>; leaf: string } {
  const expand = new Set<string>()
  let key = ''
  let cur = root
  for (;;) {
    let best = -1
    for (let i = 0; i < cur.children.length; i++) {
      const c = cur.children[i]
      if (c.total > 0 && (best < 0 || c.total > cur.children[best].total)) best = i
    }
    if (best < 0) return { expand, leaf: key }
    expand.add(key)
    key = childKey(key, best)
    cur = cur.children[best]
  }
}

export function parentKey(key: string): string | null {
  if (key === '') return null
  const i = key.lastIndexOf('/')
  return i < 0 ? '' : key.slice(0, i)
}

export function toggled(set: ReadonlySet<string>, key: string): Set<string> {
  const next = new Set(set)
  if (!next.delete(key)) next.add(key)
  return next
}

export function moveSelection(
  rows: TreeRow[],
  current: string | null,
  dir: 'up' | 'down' | 'first' | 'last',
): string | null {
  const nodes = rows.filter((r) => r.kind === 'node')
  if (nodes.length === 0) return null
  if (dir === 'first') return nodes[0].key
  if (dir === 'last') return nodes[nodes.length - 1].key
  const i = nodes.findIndex((r) => r.key === current)
  if (i < 0) return nodes[0].key
  const j = dir === 'down' ? Math.min(i + 1, nodes.length - 1) : Math.max(i - 1, 0)
  return nodes[j].key
}
