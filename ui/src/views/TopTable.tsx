import { useMemo, useState } from 'react'
import type { TopRow } from '../api'
import { formatPct, formatValue } from '../format'

type SortKey = 'selfValue' | 'total'

export function TopTable({
  rows,
  unit,
  onSelect,
}: {
  rows: TopRow[]
  unit: string
  onSelect: (funcId: number) => void
}) {
  const [key, setKey] = useState<SortKey>('selfValue')
  const sorted = useMemo(() => [...rows].sort((a, b) => b[key] - a[key]), [rows, key])
  const th = (k: SortKey, label: string) => (
    <th className="num" aria-sort={key === k ? 'descending' : 'none'}>
      <button onClick={() => setKey(k)}>
        {label}
        {key === k ? ' ▼' : ''}
      </button>
    </th>
  )
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Function</th>
            {th('selfValue', 'Self')}
            <th className="num">Self %</th>
            {th('total', 'Total')}
            <th className="num">Total %</th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((r) => (
            <tr key={r.funcId}>
              <td className="name" title={r.file ? `${r.name}\n${r.file}` : r.name}>
                <button className="linklike mono" onClick={() => onSelect(r.funcId)}>
                  {r.name}
                </button>
              </td>
              <td className="num">{formatValue(r.selfValue, unit)}</td>
              <td className="num">{formatPct(r.selfPct)}</td>
              <td className="num">{formatValue(r.total, unit)}</td>
              <td className="num">{formatPct(r.totalPct)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <p className="muted">No rows.</p>}
    </div>
  )
}
