import { useEffect, useState } from 'react'
import { ErrorNotice } from './ErrorNotice'
import { reloadPage } from './reload'
import { getFindings, type AnalysisQuery, type ApiError, type Finding } from '../api'
import { actionToPatch, loadDisabled, saveDisabled, visibleFindings } from '../analysis/findings'
import { formatPct } from '../format'
import type { ViewState } from '../url'

type Props = {
  profileRef: string
  query: AnalysisQuery
  navigate: (patch: Partial<ViewState>, mode?: 'push' | 'replace') => void
}

const SEVERITY_LABEL = { info: 'Info', warn: 'Warning', high: 'High' } as const

export function Findings({ profileRef, query, navigate }: Props) {
  const [all, setAll] = useState<Finding[]>([])
  const [error, setError] = useState<unknown>(null)
  const [disabled, setDisabled] = useState<Set<string>>(loadDisabled)
  const { st, focus, exclude, hide, tx, t0, t1 } = query

  useEffect(() => {
    let stale = false
    getFindings(profileRef, { st, focus, exclude, hide, tx, t0, t1 }).then(
      (f) => { if (!stale) { setAll(f); setError(null) } },
      (e: ApiError) => { if (!stale) { setAll([]); setError(e) } },
    )
    return () => { stale = true }
  }, [profileRef, st, focus, exclude, hide, tx, t0, t1])

  const toggle = (rule: string, enabled: boolean) => {
    const next = new Set(disabled)
    if (enabled) next.delete(rule)
    else next.add(rule)
    setDisabled(next)
    saveDisabled(next)
  }
  const shown = visibleFindings(all, disabled)
  const rules = [...new Set([...all.map((f) => f.rule), ...disabled])].sort()

  return (
    <section className="card findings" aria-label="Findings">
      <h3>Findings</h3>
      <ErrorNotice error={error} onRetry={reloadPage} />
      {shown.length === 0 ? (
        <p className="muted">No findings for this view.</p>
      ) : (
        <ul>
          {shown.map((f) => (
            <li key={f.id} className={`finding sev-${f.severity}`}>
              <span className="sev">{SEVERITY_LABEL[f.severity]}</span> <strong>{f.title}</strong>
              <p>{f.detail}</p>
              <p className="muted">
                {f.evidence.name ? `${f.evidence.name}: ` : ''}
                {formatPct(f.evidence.pct)}
              </p>
              <button aria-label={`${f.action.label}: ${f.title}`} onClick={() => navigate(actionToPatch(f.action))}>
                {f.action.label}
              </button>
            </li>
          ))}
        </ul>
      )}
      {rules.length > 0 && (
        <details>
          <summary>Manage rules</summary>
          {rules.map((r) => (
            <label key={r} className="field" style={{ flexDirection: 'row', gap: 6 }}>
              <input type="checkbox" aria-label={r} checked={!disabled.has(r)} onChange={(e) => toggle(r, e.target.checked)} />
              {r}
            </label>
          ))}
        </details>
      )}
    </section>
  )
}
