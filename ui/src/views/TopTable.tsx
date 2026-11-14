import type { TopRow } from '../api'
import { formatPct, formatValue } from '../format'
import { Term } from '../help/Term'

export function TopTable({
  rows,
  unit,
  sort,
  onSort,
  onSelect,
}: {
  rows: TopRow[]
  unit: string
  sort: 'self' | 'total'
  onSort: (s: 'self' | 'total') => void
  onSelect: (funcId: number) => void
}) {
  const th = (k: 'self' | 'total', label: string) => (
    <th className="num" aria-sort={sort === k ? 'descending' : 'none'}>
      <button onClick={() => onSort(k)}>
        {label}
        {sort === k ? ' ▼' : ''}
      </button>
      <Term k={k} icon />
    </th>
  )
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Function</th>
            {th('self', 'Self')}
            <th className="num">Self %</th>
            {th('total', 'Total')}
            <th className="num">Total %</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
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
