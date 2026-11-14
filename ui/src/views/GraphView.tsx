import { useEffect, useMemo, useRef, useState } from 'react'
import type { CallGraph, GraphNode } from '../api'
import type { Transform } from '../analysis/transforms'
import { formatPct, formatValue } from '../format'
import { layoutGraph, type Route } from '../graph/layout'
import { edgeWidth, heat, moveFocus, neighbours, nodeSize, type Dir } from '../graph/model'

type Props = {
  graph: CallGraph
  unit: string
  nodes: number
  onNodes: (n: number) => void
  onOpenFunc: (funcId: number) => void
  onTransform: (t: Transform) => void
}

const ARROW_DIRS: Record<string, Dir> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' }
const MAX_LABEL = 34
const PAD = 20

function pathOf(r: Route): string {
  const p = r.points
  if (p.length < 2) return ''
  let d = `M${p[0][0]} ${p[0][1]}`
  for (let i = 1; i < p.length; i++) {
    const mid = (p[i - 1][1] + p[i][1]) / 2
    d += ` C${p[i - 1][0]} ${mid} ${p[i][0]} ${mid} ${p[i][0]} ${p[i][1]}`
  }
  return d
}

const clip = (s: string) => (s.length > MAX_LABEL ? `${s.slice(0, MAX_LABEL - 1)}…` : s)

export function GraphView({ graph, unit, nodes, onNodes, onOpenFunc, onTransform }: Props) {
  const [selected, setSelected] = useState<number | null>(null)
  const [view, setView] = useState({ x: PAD, y: PAD, k: 1 })
  const [slider, setSlider] = useState(nodes)
  const boxRef = useRef<HTMLDivElement>(null)
  const nodeRefs = useRef(new Map<number, SVGGElement>())
  const drag = useRef<{ px: number; py: number; x: number; y: number } | null>(null)

  useEffect(() => setSlider(nodes), [nodes])

  const byId = useMemo(() => new Map(graph.nodes.map((n) => [n.funcId, n])), [graph])
  const sizes = useMemo(
    () => new Map(graph.nodes.map((n) => [n.funcId, nodeSize(n, graph.totalValue)])),
    [graph],
  )
  const layout = useMemo(
    () =>
      layoutGraph(
        graph.nodes.map((n) => ({ id: n.funcId, ...sizes.get(n.funcId)! })),
        graph.edges.map((e) => ({ from: e.from, to: e.to, weight: e.value })),
      ),
    [graph, sizes],
  )
  const maxEdge = useMemo(() => graph.edges.reduce((m, e) => Math.max(m, e.value), 0), [graph])
  const edgeMeta = useMemo(() => {
    const m = new Map<string, { value: number; elided: boolean }>()
    for (const e of graph.edges) m.set(`${e.from}>${e.to}`, e)
    return m
  }, [graph])

  const fit = () => {
    const w = boxRef.current?.clientWidth ?? 800
    const k = layout.width > 0 ? Math.min(1, (w - 2 * PAD) / layout.width) : 1
    setView({ x: PAD + Math.max(0, (w - 2 * PAD - layout.width * k) / 2), y: PAD, k: Math.max(0.1, k) })
  }
  useEffect(() => {
    fit()
    setSelected(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layout])

  // Non-passive wheel so zoom does not also scroll the page.
  useEffect(() => {
    const el = boxRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const f = e.deltaY < 0 ? 1.1 : 1 / 1.1
      setView((v) => ({ ...v, k: Math.min(4, Math.max(0.1, v.k * f)) }))
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [])

  useEffect(() => {
    if (selected !== null) nodeRefs.current.get(selected)?.focus()
  }, [selected])

  if (graph.nodes.length === 0 || graph.totalValue <= 0) return <p className="muted">No samples in this view.</p>

  const placedById = new Map(layout.nodes.map((p) => [p.id, p]))
  const zoom = (f: number) => setView((v) => ({ ...v, k: Math.min(4, Math.max(0.1, v.k * f)) }))
  const label = (n: GraphNode) =>
    `${n.name}, self ${formatValue(n.selfValue, unit)} (${formatPct((n.selfValue / graph.totalValue) * 100)}), total ${formatValue(n.total, unit)} (${formatPct((n.total / graph.totalValue) * 100)})`

  const onKeyDown = (e: React.KeyboardEvent) => {
    const dir = ARROW_DIRS[e.key]
    if (dir) {
      e.preventDefault()
      setSelected(moveFocus(graph, layout.nodes, selected, dir))
    } else if (e.key === 'Enter' && selected !== null) {
      e.preventDefault()
      onOpenFunc(selected)
    } else if (e.key === 'f' && selected !== null && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault()
      const n = byId.get(selected)
      if (n) onTransform({ kind: 'sub', name: n.name })
    }
  }

  const sel = selected !== null ? byId.get(selected) : undefined
  const nb = selected !== null ? neighbours(graph, selected) : null
  // Roving tabindex: the selected node, or the first node when nothing is selected.
  const tabbable = selected ?? graph.nodes[0].funcId

  return (
    <section className="graph" aria-label="Call graph">
      <div className="toolbar">
        <label className="field">
          Nodes: {slider}
          <input
            type="range"
            min={5}
            max={300}
            value={slider}
            aria-label="Nodes"
            onChange={(e) => setSlider(Number(e.target.value))}
            onPointerUp={() => slider !== nodes && onNodes(slider)}
            onKeyUp={() => slider !== nodes && onNodes(slider)}
          />
        </label>
        <button onClick={() => zoom(1.25)} aria-label="Zoom in">+</button>
        <button onClick={() => zoom(0.8)} aria-label="Zoom out">−</button>
        <button onClick={fit} aria-label="Fit">Fit</button>
        {graph.hiddenNodes > 0 && <span className="muted">{graph.hiddenNodes.toLocaleString('en-US')} more functions hidden</span>}
      </div>
      <div
        ref={boxRef}
        className="graph-box"
        onKeyDown={onKeyDown}
        onPointerDown={(e) => {
          if ((e.target as Element).closest('[data-node]')) return
          drag.current = { px: e.clientX, py: e.clientY, x: view.x, y: view.y }
          ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
        }}
        onPointerMove={(e) => {
          const d = drag.current
          if (d) setView((v) => ({ ...v, x: d.x + e.clientX - d.px, y: d.y + e.clientY - d.py }))
        }}
        onPointerUp={() => { drag.current = null }}
      >
        <svg width="100%" height="100%" role="group" aria-label="Call graph nodes. Arrow keys move, Enter opens the function, f focuses its subtree.">
          <defs>
            <marker id="graph-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerUnits="userSpaceOnUse" markerWidth="9" markerHeight="9" orient="auto-start-reverse">
              <path d="M0 0 L8 4 L0 8 z" fill="currentColor" />
            </marker>
          </defs>
          <g transform={`translate(${view.x} ${view.y}) scale(${view.k})`}>
            {layout.routes.map((r, i) => {
              const meta = edgeMeta.get(`${r.from}>${r.to}`)
              return (
                <path
                  key={i}
                  className="graph-edge"
                  d={pathOf(r)}
                  fill="none"
                  stroke="currentColor"
                  strokeOpacity={0.55}
                  strokeWidth={edgeWidth(meta?.value ?? 0, maxEdge)}
                  strokeDasharray={meta?.elided ? '5 4' : undefined}
                  markerEnd="url(#graph-arrow)"
                  aria-hidden="true"
                />
              )
            })}
            {graph.nodes.map((n) => {
              const p = placedById.get(n.funcId)
              const s = sizes.get(n.funcId)!
              if (!p) return null
              const h = heat(n, graph.totalValue)
              return (
                <g
                  key={n.funcId}
                  data-node
                  ref={(el) => { if (el) nodeRefs.current.set(n.funcId, el); else nodeRefs.current.delete(n.funcId) }}
                  className={`graph-node${n.funcId === selected ? ' selected' : ''}`}
                  transform={`translate(${p.x} ${p.y})`}
                  role="button"
                  tabIndex={n.funcId === tabbable ? 0 : -1}
                  aria-label={label(n)}
                  aria-pressed={n.funcId === selected}
                  onClick={() => setSelected(n.funcId)}
                  onDoubleClick={() => onOpenFunc(n.funcId)}
                  onFocus={() => selected !== n.funcId && setSelected(n.funcId)}
                >
                  <rect width={s.w} height={s.h} rx={4} fill={`hsl(24 78% ${Math.round(92 - 40 * h)}%)`} />
                  <text x={6} y={15} className="graph-name">{clip(n.name)}</text>
                  <text x={6} y={s.h - 9} className="graph-nums">
                    self {formatPct((n.selfValue / graph.totalValue) * 100)} · total {formatPct((n.total / graph.totalValue) * 100)}
                  </text>
                </g>
              )
            })}
          </g>
        </svg>
      </div>
      {sel && nb && (
        <div className="graph-side card" aria-label="Selected function">
          <h3 className="mono">{sel.name}</h3>
          <p className="muted">self {formatValue(sel.selfValue, unit)} · total {formatValue(sel.total, unit)}</p>
          <div>
            <strong>Callers</strong>
            <ul>
              {nb.callers.length === 0 && <li className="muted">none shown</li>}
              {nb.callers.map((id) => (
                <li key={id}><button className="linklike mono" onClick={() => setSelected(id)}>{byId.get(id)?.name}</button></li>
              ))}
            </ul>
          </div>
          <div>
            <strong>Callees</strong>
            <ul>
              {nb.callees.length === 0 && <li className="muted">none shown</li>}
              {nb.callees.map((id) => (
                <li key={id}><button className="linklike mono" onClick={() => setSelected(id)}>{byId.get(id)?.name}</button></li>
              ))}
            </ul>
          </div>
          <button onClick={() => onOpenFunc(sel.funcId)}>Open in sandwich</button>
        </div>
      )}
    </section>
  )
}
