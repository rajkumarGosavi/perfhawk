import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { FlameNode } from '../api'
import { formatPct, formatValue } from '../format'
import { layoutFlame, layoutHeight, type Rect } from './layout'

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
}

export function FlameGraph({ root, unit, inverted, search, onSelectFunc }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [width, setWidth] = useState(0)
  const [zoom, setZoom] = useState<FlameNode | undefined>(undefined)
  const [hover, setHover] = useState<{ rect: Rect; px: number; py: number } | null>(null)
  const [theme, setTheme] = useState(0)

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
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const f = () => setTheme((t) => t + 1)
    mq.addEventListener('change', f)
    return () => mq.removeEventListener('change', f)
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setZoom(undefined)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const rects = useMemo(
    () => layoutFlame(root, width, ROW_HEIGHT, inverted, zoom),
    [root, width, inverted, zoom],
  )
  const height = layoutHeight(rects, ROW_HEIGHT)
  const needle = search?.trim().toLowerCase()

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || width <= 0) return
    const dpr = window.devicePixelRatio || 1
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
    ctx.font = '11px ui-monospace, Consolas, monospace'
    ctx.textBaseline = 'middle'
    for (const r of rects) {
      const isMatch = !!needle && r.node.name.toLowerCase().includes(needle)
      ctx.fillStyle = isMatch ? match : `hsl(${hashHue(r.node.name)} ${sat} ${light})`
      ctx.fillRect(r.x, r.y, Math.max(r.w - 1, 0.5), ROW_HEIGHT - 1)
      if (r.w > LABEL_MIN_W) {
        ctx.fillStyle = isMatch ? '#ffffff' : text
        ctx.save()
        ctx.beginPath()
        ctx.rect(r.x, r.y, r.w - 1, ROW_HEIGHT)
        ctx.clip()
        ctx.fillText(fit(ctx, r.node.name, r.w - 6), r.x + 3, r.y + ROW_HEIGHT / 2)
        ctx.restore()
      }
    }
  }, [rects, width, height, needle, theme])

  const hit = useCallback(
    (e: { clientX: number; clientY: number }) => {
      const c = canvasRef.current
      if (!c) return null
      const b = c.getBoundingClientRect()
      const x = e.clientX - b.left
      const y = e.clientY - b.top
      const rect = rects.find((r) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + ROW_HEIGHT)
      return rect ? { rect, px: x, py: y } : null
    },
    [rects],
  )

  const t = hover?.rect.node
  return (
    <div className="flame-wrap" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={`${inverted ? 'Icicle' : 'Flame'} graph of ${root.name}, ${formatValue(root.total, unit)} total. Click a frame to zoom, Escape to reset.`}
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
