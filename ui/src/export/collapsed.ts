import type { FlameNode } from '../api'

/** Collapsed stacks: `a;b;c value` per node with self cost, pre-order. The root name is omitted unless it is the only frame. Iterative. */
export function toCollapsed(root: FlameNode): string {
  const lines: string[] = []
  const names: string[] = []
  type Item = { node: FlameNode; exit: boolean; pushed: boolean }
  const stack: Item[] = [{ node: root, exit: false, pushed: false }]
  while (stack.length) {
    const it = stack.pop()!
    if (it.exit) {
      if (it.pushed) names.pop()
      continue
    }
    const { node } = it
    const pushed = !(node === root && node.children.length > 0)
    if (pushed) names.push(node.name)
    if (node.selfValue > 0) lines.push(`${names.join(';')} ${node.selfValue}`)
    stack.push({ node, exit: true, pushed })
    for (let i = node.children.length - 1; i >= 0; i--) {
      stack.push({ node: node.children[i], exit: false, pushed: false })
    }
  }
  return lines.length ? lines.join('\n') + '\n' : ''
}
