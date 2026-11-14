import { useState } from 'react'
import { getCheck, type CheckReport } from '../api'
import { formatValue } from '../format'
import { ErrorNotice } from './ErrorNotice'
import {
  DEFAULT_MIN_VALUE, DEFAULT_THRESHOLD, checkCommand, checkSummary, parseNonNegative,
} from './checkLogic'

/** The UI form of `perfhawk check`: does the head profile regress against the baseline? */
export function CheckPanel({
  head,
  base,
  sampleType,
  onFocusFunction,
}: {
  head: string
  base: string
  sampleType?: string
  onFocusFunction?: (name: string) => void
}) {
  const [threshold, setThreshold] = useState(String(DEFAULT_THRESHOLD))
  const [minValue, setMinValue] = useState(String(DEFAULT_MIN_VALUE))
  const [report, setReport] = useState<CheckReport | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const t = parseNonNegative(threshold)
  const m = parseNonNegative(minValue)
  const edit = (set: (v: string) => void) => (e: { target: { value: string } }) => {
    set(e.target.value)
    setReport(null)
    setProblem(null)
  }

  const run = async () => {
    setError(null)
    if (t === null || m === null) {
      setProblem('Threshold and minimum value must be numbers, 0 or more.')
      return
    }
    setProblem(null)
    setBusy(true)
    try {
      setReport(await getCheck(head, base, { threshold: t, min_value: m, st: sampleType }))
    } catch (e) {
      setReport(null)
      setError(e)
    } finally {
      setBusy(false)
    }
  }

  const unit = report?.sampleType.unit ?? 'count'
  const cmd = checkCommand(head, base, t ?? DEFAULT_THRESHOLD, m ?? DEFAULT_MIN_VALUE, sampleType)
  return (
    <div className="card">
      <p className="muted">
        Fails when a function&apos;s own cost grows by more than the threshold and its share of the profile grows by
        at least the minimum. Improvements never fail.
      </p>
      <div className="toolbar">
        <label className="field">
          Threshold (% growth)
          <input inputMode="decimal" value={threshold} onChange={edit(setThreshold)} />
        </label>
        <label className="field">
          Minimum value (share points)
          <input inputMode="decimal" value={minValue} onChange={edit(setMinValue)} />
        </label>
        <button type="button" onClick={() => void run()} disabled={busy}>Run check</button>
      </div>
      {problem && <p className="banner" role="alert">{problem}</p>}
      <ErrorNotice error={error} />
      <p role="status">
        {report && <strong className={report.ok ? 'down' : 'up'}>{checkSummary(report.ok, report.regressions.length)}</strong>}
      </p>
      {report && report.regressions.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Function</th><th className="num">Base self</th><th className="num">Head self</th>
                <th className="num">Growth</th><th className="num">Share change</th>
              </tr>
            </thead>
            <tbody>
              {report.regressions.map((r) => (
                <tr key={r.name}>
                  <td className="name" title={r.name}>
                    {onFocusFunction ? (
                      <button type="button" className="linklike" onClick={() => onFocusFunction(r.name)}>{r.name}</button>
                    ) : (
                      r.name
                    )}
                  </td>
                  <td className="num">{formatValue(r.baseSelf, unit)}</td>
                  <td className="num">{formatValue(r.headSelf, unit)}</td>
                  <td className="num">{r.growthPct === null ? 'new' : `${r.growthPct >= 0 ? '+' : ''}${r.growthPct.toFixed(0)}%`}</td>
                  <td className="num">{`${r.deltaPoints >= 0 ? '+' : ''}${r.deltaPoints.toFixed(1)} pts`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="muted">
        Or from a terminal: <code className="mono">{cmd}</code>
      </p>
    </div>
  )
}
