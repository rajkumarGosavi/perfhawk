import { useEffect, useState } from 'react'
import { getDoctor, type Diagnosis } from '../api'
import { confidenceLabel, isFiniteShare, verdictTitle } from '../analysis/doctor'

type Props = { sessionId: string; onOpenProfile: (shortId: string) => void }

function copy(text: string) {
  try {
    void navigator.clipboard?.writeText(text)
  } catch {
    /* clipboard unavailable: the command is still on screen */
  }
}

export function Doctor({ sessionId, onOpenProfile }: Props) {
  const [d, setD] = useState<Diagnosis | null>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    let live = true
    getDoctor(sessionId).then(
      (r) => live && setD(r),
      () => live && setFailed(true),
    )
    return () => {
      live = false
    }
  }, [sessionId])

  if (failed) return <div className="muted">Doctor is not available for this session</div>
  if (!d) return null
  return (
    <div className="doctor">
      <h3>
        Doctor: {verdictTitle(d.verdict)}
      </h3>
      <p>{d.summary}</p>
      <ul className="doctor-findings">
        {d.findings.map((f) => (
          <li key={f.rule}>
            <strong>{f.title}</strong>{' '}
            <span className="muted">
              {confidenceLabel(f.confidence, f.fidelity)}
              {isFiniteShare(f.sharePct) ? `, ${f.sharePct.toFixed(0)}%` : ''}
            </span>
            {f.next.map((n, i) => (
              <div key={i} className="doctor-next">
                {n.command ? <code>{n.command}</code> : <span>{n.label}</span>}{' '}
                {n.command && <button onClick={() => copy(n.command as string)}>Copy</button>}{' '}
                {n.openProfile && <button onClick={() => onOpenProfile(n.openProfile as string)}>Open view</button>}
              </div>
            ))}
            {f.hints.map((h) => (
              <div key={h} className="muted">{h}</div>
            ))}
          </li>
        ))}
      </ul>
      {d.notes.map((n) => (
        <div key={n} className="muted">{n}</div>
      ))}
    </div>
  )
}
