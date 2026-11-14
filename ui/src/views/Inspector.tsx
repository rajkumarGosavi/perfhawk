import { useEffect, useState } from 'react'
import { getSandwich, type AnalysisQuery, type FlameNode, type Sandwich } from '../api'
import type { Transform } from '../analysis/transforms'
import { formatPct, formatValue } from '../format'
import { exactRegex, type FilterKind } from './filterModel'
import { topChildren, type ChildShare } from './inspectorModel'

type Props = {
  node: FlameNode | null
  rootTotal: number
  unit: string
  profileRef: string
  query: AnalysisQuery
  onTransform: (t: Transform) => void
  onFilter: (kind: FilterKind, value: string) => void
  onSandwich: (funcId: number) => void
}

function Shares({ title, rows, unit }: { title: string; rows: ChildShare[]; unit: string }) {
  return (
    <div className="inspector-list">
      <h4>{title}</h4>
      {rows.length === 0 ? (
        <p className="muted">None</p>
      ) : (
        <ul>
          {rows.map((r, i) => (
            <li key={`${r.name}-${i}`}>
              <span className="mono inspector-fn">{r.name}</span>
              <span className="inspector-bar" aria-hidden="true"><span style={{ width: `${Math.min(100, r.pct)}%` }} /></span>
              <span className="num">{formatPct(r.pct)}</span>
              <span className="visually-hidden"> ({formatValue(r.total, unit)})</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Side panel for the selected flame frame: numbers, actions, direct callers and callees. */
export function Inspector({ node, rootTotal, unit, profileRef, query, onTransform, onFilter, onSandwich }: Props) {
  const [sand, setSand] = useState<Sandwich | null>(null)
  const [failed, setFailed] = useState(false)
  const funcId = node?.funcId ?? null
  const key = JSON.stringify(query)

  useEffect(() => {
    setSand(null)
    setFailed(false)
    if (funcId === null) return
    let stale = false
    getSandwich(profileRef, funcId, query).then(
      (s) => { if (!stale) setSand(s) },
      () => { if (!stale) setFailed(true) },
    )
    return () => { stale = true }
    // `key` stands for the query contents.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileRef, funcId, key])

  return (
    <aside className="inspector" aria-label="Inspector">
      {node === null ? (
        <p className="muted">Select a frame to inspect it.</p>
      ) : (
        <>
          <h3 className="inspector-name mono">{node.name}</h3>
          {node.file && <p className="muted mono inspector-file">{node.file}</p>}
          <div className="inspector-cards">
            <div className="inspector-card">
              <span className="muted">Self</span>
              <strong>{formatPct(rootTotal > 0 ? (node.selfValue / rootTotal) * 100 : 0)}</strong>
              <span className="muted">{formatValue(node.selfValue, unit)}</span>
            </div>
            <div className="inspector-card">
              <span className="muted">Total</span>
              <strong>{formatPct(rootTotal > 0 ? (node.total / rootTotal) * 100 : 0)}</strong>
              <span className="muted">{formatValue(node.total, unit)}</span>
            </div>
          </div>
          <div className="inspector-actions" role="group" aria-label="Actions for the selected frame">
            <button type="button" onClick={() => onTransform({ kind: 'sub', name: node.name })}>Focus subtree</button>
            <button type="button" onClick={() => onTransform({ kind: 'merge', name: node.name })}>Merge</button>
            <button type="button" onClick={() => onTransform({ kind: 'drop', name: node.name })}>Drop</button>
            <button type="button" onClick={() => onFilter('hide', exactRegex(node.name))}>Hide</button>
            <button type="button" disabled={funcId === null} onClick={() => funcId !== null && onSandwich(funcId)}>Sandwich</button>
          </div>
          {failed && <p className="muted" role="status">Could not load callers and callees.</p>}
          {!failed && funcId !== null && !sand && <p className="muted" role="status">Loading callers and callees</p>}
          {sand && (
            <>
              <Shares title="Callers" rows={topChildren(sand.callers, 5)} unit={unit} />
              <Shares title="Callees" rows={topChildren(sand.callees, 5)} unit={unit} />
            </>
          )}
        </>
      )}
    </aside>
  )
}
