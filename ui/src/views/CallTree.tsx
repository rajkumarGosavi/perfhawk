import { useEffect, useMemo, useRef, useState } from 'react'
import type { FlameNode } from '../api'
import { hotPathKeys, moveSelection, parentKey, toggled, visibleRows } from '../analysis/callTree'
import { hotPath } from '../analysis/hotPath'
import type { Transform } from '../analysis/transforms'
import { formatPct, formatValue } from '../format'

type Props = {
  root: FlameNode
  unit: string
  hot: boolean
  onSelectFunc: (funcId: number) => void
  onTransform?: (t: Transform) => void
  extra?: { header: string; cell: (node: FlameNode) => string }
}

const MAX_INDENT_LEVELS = 40
const rowId = (key: string) => `tree-row-${key}`

export function CallTree({ root, unit, hot, onSelectFunc, onTransform, extra }: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(['']))
  const [selected, setSelected] = useState<string | null>(null)
  const boxRef = useRef<HTMLDivElement>(null)

  const rows = useMemo(() => visibleRows(root, expanded), [root, expanded])
  const hotNodes = useMemo(() => new Set(hotPath(root)), [root])

  // A new tree invalidates keys; hot=1 expands the hottest path and selects its end.
  useEffect(() => {
    if (hot) {
      const { expand, leaf } = hotPathKeys(root)
      setExpanded(new Set(['', ...expand]))
      setSelected(leaf)
    } else {
      setExpanded(new Set(['']))
      setSelected(null)
    }
  }, [root, hot])

  useEffect(() => {
    if (selected === null) return
    boxRef.current?.querySelector(`[data-key="${selected}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [selected, rows])

  const grid = extra ? { gridTemplateColumns: 'minmax(200px, 1fr) 90px 90px 80px 110px' } : undefined
  const pct = (v: number) => (root.total > 0 ? formatPct((v / root.total) * 100) : '0%')
  const current = rows.find((r) => r.kind === 'node' && r.key === selected)

  const onKeyDown = (e: React.KeyboardEvent) => {
    const nav = (dir: 'up' | 'down' | 'first' | 'last') => {
      e.preventDefault()
      setSelected(moveSelection(rows, selected, dir))
    }
    if (onTransform && current && current.kind === 'node' && current.node.funcId !== null) {
      const k = e.key === 'f' ? 'sub' : e.key === 'm' ? 'merge' : e.key === 'x' ? 'drop' : null
      if (k && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault()
        onTransform({ kind: k, name: current.node.name })
        return
      }
    }
    switch (e.key) {
      case 'ArrowDown': return nav('down')
      case 'ArrowUp': return nav('up')
      case 'Home': return nav('first')
      case 'End': return nav('last')
      case 'ArrowRight': {
        if (!current || current.kind !== 'node') return
        e.preventDefault()
        if (current.hasChildren && !current.expanded) setExpanded(toggled(expanded, current.key))
        else if (current.hasChildren) setSelected(moveSelection(rows, current.key, 'down'))
        return
      }
      case 'ArrowLeft': {
        if (!current || current.kind !== 'node') return
        e.preventDefault()
        if (current.expanded) setExpanded(toggled(expanded, current.key))
        else {
          const p = parentKey(current.key)
          if (p !== null) setSelected(p)
        }
        return
      }
      case 'Enter': {
        if (current && current.kind === 'node' && current.node.funcId !== null) {
          e.preventDefault()
          onSelectFunc(current.node.funcId)
        }
        return
      }
    }
  }

  if (root.total <= 0) return <p className="muted">No samples in this view.</p>

  return (
    <div
      ref={boxRef}
      className="tree"
      role="tree"
      tabIndex={0}
      aria-label="Call tree. Arrow keys move, right and left expand and collapse, Enter opens the function."
      aria-activedescendant={selected !== null ? rowId(selected) : undefined}
      onKeyDown={onKeyDown}
    >
      <div className="tree-head" aria-hidden="true" style={grid}>
        <span className="tree-fn">Function</span>
        <span className="num">Self</span>
        <span className="num">Total</span>
        <span className="num">Total %</span>
        {extra && <span className="num">{extra.header}</span>}
      </div>
      {rows.map((r) => {
        const indent = { paddingLeft: Math.min(r.depth, MAX_INDENT_LEVELS) * 14 + 4 }
        if (r.kind === 'more') {
          return (
            <div key={r.key} className="tree-row tree-more muted" role="none" style={indent}>
              … {r.count.toLocaleString('en-US')} more (use Focus to narrow)
            </div>
          )
        }
        const isHot = hotNodes.has(r.node)
        return (
          <div
            key={r.key}
            id={rowId(r.key)}
            data-key={r.key}
            className={`tree-row${r.key === selected ? ' selected' : ''}${isHot ? ' hot' : ''}`}
            role="treeitem"
            aria-level={r.depth + 1}
            aria-expanded={r.hasChildren ? r.expanded : undefined}
            aria-selected={r.key === selected}
            style={grid}
            onClick={() => setSelected(r.key)}
          >
            <span className="tree-fn" style={indent}>
              {r.hasChildren ? (
                <button
                  className="tree-chevron"
                  tabIndex={-1}
                  aria-hidden="true"
                  onClick={(e) => {
                    e.stopPropagation()
                    setSelected(r.key)
                    setExpanded(toggled(expanded, r.key))
                  }}
                >
                  {r.expanded ? '▾' : '▸'}
                </button>
              ) : (
                <span className="tree-chevron" aria-hidden="true" />
              )}
              <span className="mono">{r.node.name}</span>
              {r.node.truncated && <span className="muted"> (deeper frames folded)</span>}
              {isHot && <span className="sr-only">, on the hot path</span>}
            </span>
            <span className="num"><span className="sr-only">self </span>{formatValue(r.node.selfValue, unit)}</span>
            <span className="num"><span className="sr-only">total </span>{formatValue(r.node.total, unit)}</span>
            <span className="num"><span className="sr-only">of whole </span>{pct(r.node.total)}</span>
            {extra && <span className="num">{extra.cell(r.node)}</span>}
          </div>
        )
      })}
    </div>
  )
}
