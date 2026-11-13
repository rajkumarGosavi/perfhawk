import type { FlameNode } from '../api'

/** Root-to-leaf chain following the heaviest child at every level (first child wins ties). */
export function hotPath(root: FlameNode): FlameNode[] {
  const path = [root]
  let cur = root
  for (;;) {
    let best: FlameNode | null = null
    for (const c of cur.children) {
      if (c.total > 0 && (best === null || c.total > best.total)) best = c
    }
    if (best === null) return path
    path.push(best)
    cur = best
  }
}

export function hotPathEndsTruncated(path: FlameNode[]): boolean {
  return path.length > 0 && path[path.length - 1].truncated === true
}
