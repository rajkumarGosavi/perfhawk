import type { RecordJob } from '../api'

const HEADLINE: Record<RecordJob['state'], string> = {
  running: 'Recording in progress',
  done: 'Recording finished',
  cancelled: 'Recording cancelled; what was captured is kept',
  failed: 'Recording failed',
}

/** Progress of the current capture. Only the headline is announced; elapsed time and lines are not. */
export function RecordProgress({ job, onCancel }: { job: RecordJob; onCancel: () => void }) {
  return (
    <section className="card" aria-label="Recording progress">
      <p role="status" aria-live="polite" aria-atomic="true">
        <strong>{HEADLINE[job.state]}</strong>
      </p>
      <p className="muted">Elapsed: {Math.round(job.duration_ms / 1000)} s</p>
      {job.progress.length > 0 && (
        <ul className="mono muted" aria-label="Recorder messages" style={{ margin: 0, paddingLeft: '1.2em' }}>
          {job.progress.map((line, i) => (
            <li key={i}>{line}</li>
          ))}
        </ul>
      )}
      {job.state === 'running' && (
        <button type="button" className="danger" onClick={onCancel} style={{ marginTop: 8 }}>
          Cancel recording
        </button>
      )}
    </section>
  )
}
