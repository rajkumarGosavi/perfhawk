import { useEffect, useMemo, useState } from 'react'
import { getHeatmap, type Heatmap } from '../api'
import { formatRange, normalizeRange, nudge, type Range } from '../time/range'
import { heatDurationMs, heatToSeries, rangeFromDrag } from '../time/series'
import { toPath } from '../time/sparkline'

type Props = {
  profileRef: string
  query: { st?: number; focus?: string; exclude?: string; hide?: string; tx?: string }
  range: Range | undefined
  onRange: (r: Range | undefined) => void
}

const W = 1000
const H = 64

/**
 * Activity over time with a draggable selection. The chosen window narrows every view below it.
 * Drag to select, Esc / double-click / the breadcrumb to clear; arrows move and resize it from the keyboard.
 */
export function TimelineStrip({ profileRef, query, range, onRange }: Props) {
  const [heat, setHeat] = useState<Heatmap | null>(null)
  const [drag, setDrag] = useState<{ x0: number; x1: number } | null>(null)
  const { st, focus, exclude, hide, tx } = query

  useEffect(() => {
    let stale = false
    getHeatmap(profileRef, { st, focus, exclude, hide, tx, cols: 400 }).then(
      (h) => { if (!stale) setHeat(h) },
      () => { if (!stale) setHeat(null) },
    )
    return () => { stale = true }
  }, [profileRef, st, focus, exclude, hide, tx])

  const series = useMemo(() => (heat ? heatToSeries(heat) : []), [heat])
  if (!heat || heat.cells.length === 0) return null

  const duration = heatDurationMs(heat)
  const toX = (ms: number) => (duration > 0 ? (ms / duration) * W : 0)
  const shown: Range | undefined = drag
    ? rangeFromDrag(drag.x0, drag.x1, W, duration) ?? undefined
    : range
  const px = (e: { clientX: number; currentTarget: Element }) => {
    const r = e.currentTarget.getBoundingClientRect()
    return r.width > 0 ? ((e.clientX - r.left) / r.width) * W : 0
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); onRange(undefined); return }
    const dir = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'grow', ArrowDown: 'shrink' }[e.key] as
      | 'left' | 'right' | 'grow' | 'shrink' | undefined
    if (!dir) return
    e.preventDefault()
    const step = Math.max(1, Math.round(duration * 0.02))
    const base = range ?? normalizeRange(0, Math.max(step, Math.round(duration * 0.1)), duration)
    if (base) onRange(nudge(base, dir, step, duration))
  }

  return (
    <section className="timeline-strip" aria-label="Time range">
      <div className="timeline-strip-head">
        <strong>Time range</strong>
        <nav aria-label="Zoom path" className="timeline-crumbs">
          <button type="button" className="linklike" onClick={() => onRange(undefined)} disabled={!range}>Full recording</button>
          {range && <><span aria-hidden="true">&rsaquo;</span><span className="mono" aria-current="true">{formatRange(range)}</span></>}
        </nav>
        <span className="muted timeline-hint">Drag to select. Esc or double-click clears. Arrow keys move and resize.</span>
      </div>
      <svg
        className="timeline-svg"
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        role="group"
        tabIndex={0}
        aria-label={range ? `Activity over time, selected ${formatRange(range)}` : 'Activity over time, nothing selected'}
        onKeyDown={onKeyDown}
        onDoubleClick={() => onRange(undefined)}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture?.(e.pointerId)
          const x = px(e)
          setDrag({ x0: x, x1: x })
        }}
        onPointerMove={(e) => { if (drag) setDrag({ ...drag, x1: px(e) }) }}
        onPointerUp={(e) => {
          if (!drag) return
          const r = rangeFromDrag(drag.x0, px(e), W, duration)
          setDrag(null)
          if (r) onRange(r)
        }}
        onPointerCancel={() => setDrag(null)}
      >
        <path className="timeline-line" d={toPath(series, W, H - 4)} />
        {shown && (
          <>
            <rect className="timeline-band" x={toX(shown.t0)} y={0} width={Math.max(1, toX(shown.t1) - toX(shown.t0))} height={H} />
            <rect className="timeline-edge" x={toX(shown.t0)} y={0} width={3} height={H} />
            <rect className="timeline-edge" x={Math.max(0, toX(shown.t1) - 3)} y={0} width={3} height={H} />
          </>
        )}
      </svg>
    </section>
  )
}
