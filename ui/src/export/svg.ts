import type { FlameNode } from '../api'
import { layoutFlame } from '../flame/layout'

export type SvgOptions = {
  width: number
  rowHeight: number
  inverted: boolean
  /** Must return literal colours; resolve CSS variables before calling. */
  colorOf: (n: FlameNode) => string
  title: string
}

const esc = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;')

const CHAR_PX = 6.5
const HEADER = 22

/** Standalone SVG of a flame graph built from the same layout the canvas uses. */
export function flameToSvg(root: FlameNode, o: SvgOptions): string {
  const rects = layoutFlame(root, o.width, o.rowHeight, o.inverted)
  let depth = 0
  for (const r of rects) if (r.depth > depth) depth = r.depth
  const height = (depth + 1) * o.rowHeight + HEADER
  const out: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${o.width}" height="${height}" viewBox="0 0 ${o.width} ${height}" font-family="monospace" font-size="11">`,
    `<title>${esc(o.title)}</title>`,
    `<text x="4" y="14" font-size="12">${esc(o.title)}</text>`,
  ]
  for (const r of rects) {
    const name = esc(r.node.name)
    const y = r.y + HEADER
    out.push(
      `<g><title>${name} (${r.node.total})</title>` +
        `<rect data-name="${name}" x="${r.x.toFixed(2)}" y="${y.toFixed(2)}" width="${Math.max(r.w - 0.5, 0.5).toFixed(2)}" height="${o.rowHeight - 1}" fill="${esc(o.colorOf(r.node))}"/>`,
    )
    const chars = Math.floor((r.w - 6) / CHAR_PX)
    if (chars >= 3) {
      const label = r.node.name.length > chars ? r.node.name.slice(0, chars - 1) + '…' : r.node.name
      out.push(`<text x="${(r.x + 3).toFixed(2)}" y="${(y + o.rowHeight - 6).toFixed(2)}" fill="#000">${esc(label)}</text>`)
    }
    out.push('</g>')
  }
  out.push('</svg>')
  return out.join('\n')
}
