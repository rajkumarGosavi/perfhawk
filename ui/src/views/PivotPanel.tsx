import { useEffect, useState } from 'react'
import { getPivot, type PivotRow } from '../api'
import { formatPct, formatValue } from '../format'
import type { ViewState } from '../url'

type Props = {
  sessionId: string | null
  functionName: string | null
  currentProfileId: string
  navigate: (patch: Partial<ViewState>, mode?: 'push' | 'replace') => void
}

/** Where else the same function shows up in the profiles captured in this session. */
export function PivotPanel({ sessionId, functionName, currentProfileId, navigate }: Props) {
  const [rows, setRows] = useState<PivotRow[]>([])

  useEffect(() => {
    let stale = false
    if (!sessionId || !functionName) {
      setRows([])
      return
    }
    getPivot(sessionId, functionName).then(
      (r) => { if (!stale) setRows(r) },
      () => { if (!stale) setRows([]) },
    )
    return () => { stale = true }
  }, [sessionId, functionName])

  const others = rows.filter((r) => r.profileId !== currentProfileId)
  if (!sessionId || !functionName || rows.length < 2 || others.length === 0) return null

  return (
    <section className="card" aria-label="Same function in other profiles">
      <h2>Same function in other profiles</h2>
      <ul className="pivot">
        {others.map((r) => (
          <li key={r.profileId}>
            <strong>{r.kind}</strong>{' '}
            {r.found ? (
              <>
                <span>{formatPct(r.totalPct)} of total</span>{' '}
                <span className="muted">
                  (self {formatValue(r.selfValue, r.sampleType.unit)}, total {formatValue(r.total, r.sampleType.unit)})
                </span>{' '}
                <button onClick={() => navigate({ profile: r.shortId, view: 'sandwich', func: r.funcId ?? undefined }, 'push')}>
                  Open
                </button>
              </>
            ) : (
              <span className="muted">not present</span>
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
