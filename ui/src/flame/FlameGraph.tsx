import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { FlameNode } from '../api'
import { formatPct, formatValue } from '../format'
import { subscribeTheme } from '../theme'
import { layoutFlame, layoutHeight, maxCanvasRows, type Rect } from './layout'

export const ROW_HEIGHT = 18
const LABEL_MIN_W = 30

function hashHue(name: string): number {
  let h = 2166136261
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  // Warm range: red to yellow (0..55 degrees).
  return (h >>> 0) % 56
}

type Props = {
  root: FlameNode
  unit: string
  /** true = icicle (root at top), false = flame (root at bottom). */
  inverted: boolean
  search?: string
  onSelectFunc?: (funcId: number) => void
  /** Fill colour for a node (a colour or `var(--name)`); default is the hashed warm palette. */
  colorOf?: (node: FlameNode) => string
  /** Extra tooltip content for a node. */
  tooltipExtra?: (node: FlameNode) => ReactNode
  /** Shown in the tooltip of a truncated node. */
  depthLimit?: number
  /** Nodes drawn with a heavy outline (the hot path). */
  highlight?: ReadonlySet<FlameNode>
  /** Filled with export helpers (PNG of the canvas, literal colours); assigned, never state, so renders do not loop. */
  exportRef?: { current: FlameExportApi | null }
}

export type FlameExportApi = {
  toPng: () => Promise<Blob | null>
  resolveColor: (n: FlameNode) => string
}

function resolveColor(cs: CSSStyleDeclaration, c: string): string {
  const m = /^var\((--[\w-]+)\)$/.exec(c)
  return m ? cs.getPropertyValue(m[1]).trim() || '#888' : c
}

export function FlameGraph({ root, unit, inverted, search, onSelectFunc, colorOf, tooltipExtra, depthLimit = 1000, highlight, exportRef }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [width, setWidth] = useState(0)
  const [zoom, setZoom] = useState<FlameNode | undefined>(undefined)
  const [hover, setHover] = useState<{ rect: Rect; px: number; py: number } | null>(null)
  const [theme, setTheme] = useState(0)

  useEffect(() => {
    if (!exportRef) return
    exportRef.current = {
      toPng: () => new Promise((res) => {
        const c = canvasRef.current
        if (!c) res(null)
        else c.toBlob(res, 'image/png')
      }),
      resolveColor: (n) => {
        const c = canvasRef.current
        if (!c) return '#888'
        const cs = getComputedStyle(c)
        if (colorOf) return resolveColor(cs, colorOf(n))
        const sat = cs.getPropertyValue('--flame-sat').trim() || '78%'
        const light = cs.getPropertyValue('--flame-light').trim() || '62%'
        return `hsl(${hashHue(n.name)} ${sat} ${light})`
      },
    }
    return () => { exportRef.current = null }
  }, [exportRef, colorOf])

  // A new tree invalidates the zoom target.
  useEffect(() => setZoom(undefined), [root])

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(() => setWidth(Math.floor(el.clientWidth)))
    ro.observe(el)
    setWidth(Math.floor(el.clientWidth))
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    return subscribeTheme(() => setTheme((t) => t + 1))
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (e.key === 'Escape') setZoom(undefined)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const rects = useMemo(
    () => layoutFlame(root, width, ROW_HEIGHT, inverted, zoom),
    [root, width, inverted, zoom],
  )
  const fullHeight = layoutHeight(rects, ROW_HEIGHT)
  // Browsers cap canvas size, so very deep trees are clipped (deepest rows first).
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1
  const maxRows = maxCanvasRows(dpr)
  const clipped = fullHeight / ROW_HEIGHT > maxRows
  const height = clipped ? maxRows * ROW_HEIGHT : fullHeight
  const shift = clipped && !inverted ? fullHeight - height : 0
  const needle = search?.trim().toLowerCase()

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || width <= 0) return
    canvas.width = Math.floor(width * dpr)
    canvas.height = Math.floor(height * dpr)
    canvas.style.height = `${height}px`
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, width, height)
    const cs = getComputedStyle(canvas)
    const sat = cs.getPropertyValue('--flame-sat').trim() || '78%'
    const light = cs.getPropertyValue('--flame-light').trim() || '62%'
    const text = cs.getPropertyValue('--flame-text').trim() || '#000'
    const match = cs.getPropertyValue('--match').trim() || '#1e6fe0'
    const hotColor = cs.getPropertyValue('--flame-hot').trim() || '#000'
    ctx.font = '11px ui-monospace, Consolas, monospace'
    ctx.textBaseline = 'middle'
    for (const r of rects) {
      const y = r.y - shift
      if (y < 0 || y >= height) continue
      const isMatch = !!needle && r.node.name.toLowerCase().includes(needle)
      ctx.fillStyle = isMatch
        ? match
        : colorOf
          ? resolveColor(cs, colorOf(r.node))
          : `hsl(${hashHue(r.node.name)} ${sat} ${light})`
      ctx.fillRect(r.x, y, Math.max(r.w - 1, 0.5), ROW_HEIGHT - 1)
      if (highlight?.has(r.node)) {
        ctx.strokeStyle = hotColor
        ctx.lineWidth = 2
        ctx.strokeRect(r.x + 1, y + 1, Math.max(r.w - 3, 0.5), ROW_HEIGHT - 3)
      }
      if (r.node.truncated && r.w > 4) {
        // Hatch: deeper frames were folded into this node.
        ctx.save()
        ctx.beginPath()
        ctx.rect(r.x, y, r.w - 1, ROW_HEIGHT - 1)
        ctx.clip()
        ctx.strokeStyle = 'rgba(0, 0, 0, 0.45)'
        ctx.lineWidth = 1
        ctx.beginPath()
        for (let x = r.x - ROW_HEIGHT; x < r.x + r.w; x += 5) {
          ctx.moveTo(x, y + ROW_HEIGHT)
          ctx.lineTo(x + ROW_HEIGHT, y)
        }
        ctx.stroke()
        ctx.restore()
      }
      if (r.w > LABEL_MIN_W) {
        ctx.fillStyle = isMatch ? '#ffffff' : text
        ctx.save()
        ctx.beginPath()
        ctx.rect(r.x, y, r.w - 1, ROW_HEIGHT)
        ctx.clip()
        ctx.fillText(fit(ctx, r.node.name, r.w - 6), r.x + 3, y + ROW_HEIGHT / 2)
        ctx.restore()
      }
    }
  }, [rects, width, height, shift, needle, theme, colorOf, dpr, highlight])

  const hit = useCallback(
    (e: { clientX: number; clientY: number }) => {
      const c = canvasRef.current
      if (!c) return null
      const b = c.getBoundingClientRect()
      const x = e.clientX - b.left
      const y = e.clientY - b.top
      const rect = rects.find((r) => x >= r.x && x < r.x + r.w && y >= r.y - shift && y < r.y - shift + ROW_HEIGHT)
      return rect ? { rect, px: x, py: y } : null
    },
    [rects, shift],
  )

  const t = hover?.rect.node
  return (
    <div className="flame-wrap" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={`${inverted ? 'Icicle' : 'Flame'} graph of ${root.name}, ${formatValue(root.total, unit)} total. Click a frame to zoom, Escape to reset.${highlight ? ' Hot path highlighted.' : ''}`}
        onMouseMove={(e) => setHover(hit(e))}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => {
          const h = hit(e)
          if (h) setZoom(h.rect.node === root ? undefined : h.rect.node)
        }}
        onDoubleClick={() => setZoom(undefined)}
        onContextMenu={(e) => {
          const h = hit(e)
          if (h && h.rect.node.funcId !== null && onSelectFunc) {
            e.preventDefault()
            onSelectFunc(h.rect.node.funcId)
          }
        }}
      />
      {hover && t && (
        <div
          className="tooltip"
          style={{
            left: Math.min(hover.px + 12, Math.max(width - 200, 0)),
            top: hover.py + ROW_HEIGHT + 4,
          }}
        >
          <div className="fn">{t.name}</div>
          {t.file && <div className="mono">{t.file}</div>}
          <div>
            self {formatValue(t.selfValue, unit)} · total {formatValue(t.total, unit)} (
            {formatPct((t.total / (root.total || 1)) * 100)})
          </div>
          {tooltipExtra?.(t)}
          {t.truncated && <div>deeper frames folded (depth limit {depthLimit})</div>}
        </div>
      )}
      {clipped && (
        <div className="banner" role="status" style={{ margin: 4 }}>
          Graph clipped; use Focus or zoom to see deeper frames.
        </div>
      )}
      {zoom && (
        <div className="muted" style={{ padding: '2px 6px', fontSize: 12 }}>
          Zoomed to {zoom.name} ({formatPct((zoom.total / (root.total || 1)) * 100)} of total).{' '}
          <button className="linklike" onClick={() => setZoom(undefined)}>
            Reset zoom (Esc)
          </button>
        </div>
      )}
    </div>
  )
}

function fit(ctx: CanvasRenderingContext2D, s: string, maxW: number): string {
  if (ctx.measureText(s).width <= maxW) return s
  let lo = 0
  let hi = s.length
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (ctx.measureText(s.slice(0, mid) + '…').width <= maxW) lo = mid
    else hi = mid - 1
  }
  return lo > 0 ? s.slice(0, lo) + '…' : ''
}
