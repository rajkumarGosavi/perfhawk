import { useEffect, useMemo, useRef, useState } from 'react'
import type { Heatmap } from '../api'
import { cellColor, cellRect, pointToTime, timeToX } from '../time/heatLayout'
import { formatRange, normalizeRange, nudge, type Range } from '../time/range'

type Props = {
  heat: Heatmap
  range: Range | undefined
  baseline: Range | undefined
  onRange: (r: Range | undefined) => void
  onBaseline: (r: Range | undefined) => void
}

const HEIGHT = 260

/** Milliseconds covered by one cell (a row inside a column). */
const cellMs = (h: Heatmap) => Math.max(1, Math.round((h.rowBuckets * h.bucketNs) / 1e6))

export function HeatMap({ heat, range, baseline, onRange, onBaseline }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [width, setWidth] = useState(0)
  const [drag, setDrag] = useState<{ x0: number; y0: number; x1: number; y1: number; baseline: boolean } | null>(null)

  const durationMs = Math.round((heat.totalBuckets * heat.bucketNs) / 1e6)
  const colMs = Math.max(1, Math.round((heat.colBuckets * heat.bucketNs) / 1e6))

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(() => setWidth(Math.floor(el.clientWidth)))
    ro.observe(el)
    setWidth(Math.floor(el.clientWidth))
    return () => ro.disconnect()
  }, [])

  const activity = useMemo(() => {
    if (!range) return 0
    let sum = 0
    for (const c of heat.cells) {
      const ms = ((c.col * heat.colBuckets + c.row * heat.rowBuckets) * heat.bucketNs) / 1e6
      if (ms >= range.t0 && ms < range.t1) sum += c.value
    }
    return sum
  }, [heat, range])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || width <= 0) return
    const dpr = window.devicePixelRatio || 1
    canvas.width = Math.floor(width * dpr)
    canvas.height = Math.floor(HEIGHT * dpr)
    canvas.style.height = `${HEIGHT}px`
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, width, HEIGHT)
    for (const c of heat.cells) {
      const r = cellRect(heat, c, width, HEIGHT)
      const v = cellColor(c.value, heat.max)
      ctx.fillStyle = `rgba(240, 120, 40, ${0.12 + 0.88 * v})`
      ctx.fillRect(r.x, r.y, Math.max(r.w - 0.5, 0.5), Math.max(r.h - 0.5, 0.5))
    }
    const band = (r: Range, color: string, dashed: boolean, label: string) => {
      const x0 = timeToX(heat, r.t0, width)
      const x1 = timeToX(heat, r.t1, width)
      ctx.save()
      ctx.strokeStyle = color
      ctx.fillStyle = color
      ctx.lineWidth = 2
      ctx.setLineDash(dashed ? [6, 4] : [])
      ctx.strokeRect(x0, 1, Math.max(x1 - x0, 2), HEIGHT - 2)
      ctx.font = '11px ui-monospace, Consolas, monospace'
      ctx.fillText(label, x0 + 4, 12)
      ctx.restore()
    }
    if (range) band(range, '#1e6fe0', false, 'Window')
    if (baseline) band(baseline, '#9a6bd6', true, 'Baseline')
    if (drag) {
      const lo = Math.min(drag.x0, drag.x1)
      ctx.fillStyle = drag.baseline ? 'rgba(154,107,214,0.25)' : 'rgba(30,111,224,0.25)'
      ctx.fillRect(lo, 0, Math.abs(drag.x1 - drag.x0), HEIGHT)
    }
  }, [heat, width, range, baseline, drag])

  const pos = (e: React.PointerEvent) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    return { x: e.clientX - rect.left, y: e.clientY - rect.top }
  }

  const finish = (d: NonNullable<typeof drag>) => {
    const a = pointToTime(heat, d.x0, d.y0, width, HEIGHT)
    const b = pointToTime(heat, d.x1, d.y1, width, HEIGHT)
    const r = normalizeRange(Math.min(a, b), Math.max(a, b) + cellMs(heat), durationMs)
    if (d.baseline) onBaseline(r ?? undefined)
    else onRange(r ?? undefined)
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      onRange(undefined)
      onBaseline(undefined)
      return
    }
    const dir = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'grow', ArrowDown: 'shrink' }[e.key] as
      | 'left' | 'right' | 'grow' | 'shrink' | undefined
    if (!dir) return
    e.preventDefault()
    const base = range ?? normalizeRange(0, colMs, durationMs)
    if (!base) return
    const next = nudge(base, dir, colMs, durationMs)
    if (e.shiftKey) onBaseline(next)
    else onRange(next)
  }

  const setEnd = (which: 't0' | 't1', value: string) => {
    const n = Number(value)
    if (!Number.isFinite(n) || !range) return
    const r = normalizeRange(which === 't0' ? n : range.t0, which === 't1' ? n : range.t1, durationMs)
    if (r) onRange(r)
  }

  return (
    <section className="heatmap" aria-label="Time heat map">
      <p className="muted">
        Each column is {(colMs / 1000).toFixed(2)} s; rows are offsets inside it. Resolution{' '}
        {Math.round(heat.bucketNs / 1e6)} ms. Drag to select a window, Shift-drag for a baseline, Esc to clear.
        Keyboard: arrows move (Left/Right) and resize (Up/Down) the window, Shift for the baseline.
      </p>
      {heat.cells.length === 0 && <p className="muted">No activity in this capture.</p>}
      <div ref={wrapRef}>
        <canvas
          ref={canvasRef}
          className="heat-canvas"
          tabIndex={0}
          role="img"
          aria-label={`Time heat map of ${(durationMs / 1000).toFixed(1)} seconds. ${range ? `Window ${formatRange(range)}.` : 'No window selected.'}`}
          onKeyDown={onKeyDown}
          onPointerDown={(e) => {
            const p = pos(e)
            setDrag({ x0: p.x, y0: p.y, x1: p.x, y1: p.y, baseline: e.shiftKey })
            e.currentTarget.setPointerCapture(e.pointerId)
          }}
          onPointerMove={(e) => {
            if (!drag) return
            const p = pos(e)
            setDrag({ ...drag, x1: p.x, y1: p.y })
          }}
          onPointerUp={() => {
            if (drag) finish(drag)
            setDrag(null)
          }}
        />
      </div>
      <table className="heat-readout">
        <caption className="muted">Selection</caption>
        <thead>
          <tr><th>Selection</th><th>Start (ms)</th><th>End (ms)</th><th className="num">Activity</th></tr>
        </thead>
        <tbody>
          <tr>
            <td>Window</td>
            <td>
              <input
                type="number" min={0} max={durationMs} aria-label="Window start (ms)" disabled={!range}
                value={range?.t0 ?? ''} onChange={(e) => setEnd('t0', e.target.value)}
              />
            </td>
            <td>
              <input
                type="number" min={0} max={durationMs} aria-label="Window end (ms)" disabled={!range}
                value={range?.t1 ?? ''} onChange={(e) => setEnd('t1', e.target.value)}
              />
            </td>
            <td className="num">{range ? `${activity.toLocaleString('en-US')} (${formatRange(range)})` : '–'}</td>
          </tr>
          <tr>
            <td>Baseline</td>
            <td>{baseline?.t0 ?? '–'}</td>
            <td>{baseline?.t1 ?? '–'}</td>
            <td className="num">{baseline ? formatRange(baseline) : '–'}</td>
          </tr>
        </tbody>
      </table>
    </section>
  )
}
