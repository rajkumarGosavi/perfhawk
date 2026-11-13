import type { FlameNode } from '../api'

export type Rect = { x: number; y: number; w: number; depth: number; node: FlameNode }

/** Pure, iterative layout. x/w proportional to `total` within the zoomed node. */
export function layoutFlame(
  root: FlameNode,
  width: number,
  rowHeight: number,
  inverted: boolean,
  zoom?: FlameNode,
): Rect[] {
  const top = zoom ?? root
  const out: Rect[] = []
  if (top.total <= 0 || width <= 0) return out
  const scale = width / top.total
  let maxDepth = 0
  const stack: Rect[] = [{ x: 0, y: 0, w: width, depth: 0, node: top }]
  while (stack.length) {
    const r = stack.pop()!
    out.push(r)
    if (r.depth > maxDepth) maxDepth = r.depth
    let x = r.x
    for (const c of r.node.children) {
      const w = c.total * scale
      if (w >= 0.5) stack.push({ x, y: 0, w, depth: r.depth + 1, node: c })
      x += w
    }
  }
  for (const r of out) {
    r.y = inverted ? r.depth * rowHeight : (maxDepth - r.depth) * rowHeight
  }
  return out
}

export const ROW_PX = 18
/** Browsers cap canvas size; keep height x devicePixelRatio within this many device pixels. */
export const MAX_CANVAS_DEVICE_PX = 16384

export function maxCanvasRows(dpr: number): number {
  return Math.max(1, Math.floor(MAX_CANVAS_DEVICE_PX / (ROW_PX * dpr)))
}

export function layoutHeight(rects: Rect[], rowHeight: number): number {
  let d = 0
  for (const r of rects) if (r.depth > d) d = r.depth
  return (d + 1) * rowHeight
}
