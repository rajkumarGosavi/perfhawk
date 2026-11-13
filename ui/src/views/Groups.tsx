import type { GroupRow } from '../api'
import { formatPct, formatValue } from '../format'

type Props = {
  rows: GroupRow[]
  unit: string
  by: 'stack' | 'site'
  onBy: (by: 'stack' | 'site') => void
}

export function Groups({ rows, unit, by, onBy }: Props) {
  return (
    <section aria-label="Groups" className="groups">
      <div className="tabs" role="group" aria-label="Group by">
        <button aria-pressed={by === 'site'} onClick={() => onBy('site')}>By site</button>
        <button aria-pressed={by === 'stack'} onClick={() => onBy('stack')}>By identical stack</button>
      </div>
      {rows.length === 0 ? (
        <p className="muted">No samples in this view.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>{by === 'site' ? 'Site' : 'Stack (leaf first)'}</th>
              <th className="num">Value</th>
              <th className="num">Count</th>
              {by === 'site' && <th className="num">Mean</th>}
              <th className="num">%</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td>
                  {by === 'site' ? (
                    <span className="mono">{r.name}</span>
                  ) : (
                    <>
                      <strong className="mono">{r.frames[0] ?? r.name}</strong>
                      {r.frames.slice(1).map((f, j) => (
                        <span key={j} className="mono muted"> ← {f}</span>
                      ))}
                      <span className="muted"> ({r.frames.length} frames)</span>
                    </>
                  )}
                </td>
                <td className="num">{formatValue(r.value, unit)}</td>
                <td className="num">{r.count === null ? '–' : r.count.toLocaleString('en-US')}</td>
                {by === 'site' && (
                  <td className="num">{r.mean === null ? '–' : formatValue(r.mean, unit)}</td>
                )}
                <td className="num">{formatPct(r.pct)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
