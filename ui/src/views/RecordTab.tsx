import { useCallback, useEffect, useRef, useState } from 'react'
import {
  ApiError, cancelRecord, getCapabilities, getRecordCurrent, getTargets, startRecord,
  type Capabilities, type RecordJob, type TargetsResponse,
} from '../api'
import { ErrorNotice } from './ErrorNotice'
import { RecordProgress } from './RecordProgress'
import { Skeleton } from './Skeleton'
import {
  DEFAULT_DURATION_SECS, MAX_DURATION_SECS, MIN_DURATION_SECS, buildCliCommand, validateRecordForm,
  type RecordForm,
} from './recordLogic'

const POLL_MS = 1000

/** Record a running process from the browser: the UI form of `perfhawk record` and `doctor`. */
export function RecordTab({ onOpenSession }: { onOpenSession: (shortId: string, diagnose: boolean) => void }) {
  const [targets, setTargets] = useState<TargetsResponse | null>(null)
  const [targetsError, setTargetsError] = useState<unknown>(null)
  const [form, setForm] = useState<RecordForm>({ pid: '', url: '', durationSecs: String(DEFAULT_DURATION_SECS) })
  const [caps, setCaps] = useState<Capabilities | null>(null)
  const [capsNote, setCapsNote] = useState<string | null>(null)
  const [kinds, setKinds] = useState<string[]>([])
  const [name, setName] = useState('')
  const [tags, setTags] = useState('')
  const [job, setJob] = useState<RecordJob | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [diagnose, setDiagnose] = useState(false)
  const handled = useRef<string | null>(null)

  const loadTargets = useCallback(() => {
    setTargetsError(null)
    getTargets().then(setTargets, (e) => setTargetsError(e))
  }, [])
  useEffect(loadTargets, [loadTargets])

  // Re-attach to a capture that is running (or just ended) after a reload.
  useEffect(() => {
    let live = true
    getRecordCurrent().then(
      (j) => {
        if (!live) return
        if (j && j.state !== 'running') handled.current = j.job_id
        setJob(j)
      },
      (e) => live && setError(e),
    )
    return () => { live = false }
  }, [])

  const running = job?.state === 'running'
  useEffect(() => {
    if (!running) return
    const t = setInterval(() => {
      getRecordCurrent().then(setJob, (e) => setError(e))
    }, POLL_MS)
    return () => clearInterval(t)
  }, [running])

  // "Record and diagnose" opens the finished session, where Doctor is shown.
  useEffect(() => {
    if (job?.state === 'done' && job.session_id && diagnose && handled.current !== job.job_id) {
      handled.current = job.job_id
      onOpenSession(job.session_id, true)
    }
  }, [job, diagnose, onOpenSession])

  // What the chosen target can record (checked a moment after the user stops typing).
  const pid = form.pid.trim()
  const url = form.url.trim()
  const targetProblem = validateRecordForm({ pid, url, durationSecs: '1' })
  useEffect(() => {
    setKinds([])
    setCaps(null)
    setCapsNote(null)
    if (targetProblem) return
    let live = true
    const t = setTimeout(() => {
      getCapabilities(pid ? { pid: Number(pid) } : { url }).then(
        (c) => live && setCaps(c),
        (e: unknown) => live && setCapsNote(e instanceof Error ? e.message : 'Could not check this process'),
      )
    }, 400)
    return () => {
      live = false
      clearTimeout(t)
    }
  }, [pid, url, targetProblem])

  const submit = async (diag: boolean) => {
    setError(null)
    const problem = validateRecordForm(form)
    setFormError(problem)
    if (problem) return
    setDiagnose(diag)
    setBusy(true)
    try {
      await startRecord({
        ...(pid ? { pid: Number(pid) } : { url }),
        kinds: kinds.length > 0 ? kinds : undefined,
        duration_secs: Number(form.durationSecs),
        name: name.trim() || undefined,
        tags: tags.split(',').map((t) => t.trim()).filter(Boolean),
      })
      setJob(await getRecordCurrent())
    } catch (e) {
      setError(e)
      if (e instanceof ApiError && e.code === 'capture_running') getRecordCurrent().then(setJob, () => {})
    } finally {
      setBusy(false)
    }
  }

  const cancel = () => {
    if (job) cancelRecord(job.job_id).then(() => getRecordCurrent().then(setJob, () => {}), (e) => setError(e))
  }

  const cmd = pid || url ? buildCliCommand({ ...form, kinds }) : 'perfhawk record --pid <pid> --open'
  const jobError = job?.state === 'failed' && job.error ? new ApiError(job.error.code, job.error.message) : null

  return (
    <section aria-label="Record">
      {job && <RecordProgress job={job} onCancel={cancel} />}
      <ErrorNotice error={jobError} />
      {job?.state === 'done' && job.session_id && !diagnose && (
        <p>
          <button type="button" onClick={() => onOpenSession(job.session_id!, false)}>Open session</button>
        </p>
      )}

      <h3>Running processes</h3>
      <p className="muted">Go programs with the Perfhawk agent and running .NET programs on this computer.</p>
      <p>
        <button type="button" onClick={loadTargets}>Refresh list</button>
      </p>
      <ErrorNotice error={targetsError} onRetry={loadTargets} />
      {targets === null ? (
        !targetsError && <Skeleton />
      ) : targets.targets.length === 0 ? (
        <p className="muted">None found. Start your program, then refresh, or enter a pid or url below.</p>
      ) : (
        <ul className="session-profiles" aria-label="Processes you can record">
          {targets.targets.map((t) => (
            <li key={t.pid}>
              <button
                type="button"
                aria-pressed={form.pid === String(t.pid)}
                onClick={() => setForm({ ...form, pid: String(t.pid), url: '' })}
              >
                {t.label}
              </button>
            </li>
          ))}
        </ul>
      )}
      {targets?.warning && <p className="muted">{targets.warning}</p>}

      <h3>Recording settings</h3>
      <fieldset disabled={running || busy} style={{ border: 0, padding: 0, margin: 0 }}>
        <div className="toolbar">
          <label className="field">
            Process id (pid)
            <input
              inputMode="numeric"
              value={form.pid}
              onChange={(e) => setForm({ ...form, pid: e.target.value, url: e.target.value ? '' : form.url })}
            />
          </label>
          <label className="field">
            or a local pprof url
            <input
              value={form.url}
              placeholder="http://127.0.0.1:6060"
              onChange={(e) => setForm({ ...form, url: e.target.value, pid: e.target.value ? '' : form.pid })}
            />
          </label>
          <label className="field">
            Duration (seconds, {MIN_DURATION_SECS} to {MAX_DURATION_SECS})
            <input
              inputMode="numeric"
              value={form.durationSecs}
              onChange={(e) => setForm({ ...form, durationSecs: e.target.value })}
            />
          </label>
        </div>

        <fieldset style={{ border: 0, padding: 0, margin: '0 0 8px' }}>
          <legend>What to record (leave all unticked to record everything available)</legend>
          {caps ? (
            caps.kinds.map((k) => (
              <label key={k.kind} className="field check">
                <input
                  type="checkbox"
                  disabled={!k.available}
                  checked={kinds.includes(k.kind)}
                  aria-describedby={k.note ? `kind-note-${k.kind}` : undefined}
                  onChange={(e) =>
                    setKinds(e.target.checked ? [...kinds, k.kind] : kinds.filter((x) => x !== k.kind))
                  }
                />
                {k.kind}
                {k.note && <span id={`kind-note-${k.kind}`} className="muted"> ({k.note})</span>}
              </label>
            ))
          ) : (
            <span className="muted">{capsNote ?? 'Choose a process to see what it can record.'}</span>
          )}
        </fieldset>

        <details>
          <summary>More options</summary>
          <div className="toolbar">
            <label className="field">
              Session name
              <input value={name} maxLength={200} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="field">
              Tags (comma separated)
              <input value={tags} onChange={(e) => setTags(e.target.value)} />
            </label>
          </div>
        </details>

        {formError && <p className="banner" role="alert">{formError}</p>}
        <ErrorNotice error={error} />
        <p>
          <button type="button" onClick={() => void submit(false)}>Start recording</button>{' '}
          <button type="button" onClick={() => void submit(true)}>Record and diagnose</button>
        </p>
      </fieldset>
      <p className="muted">
        Or from a terminal: <code className="mono">{cmd}</code>
      </p>
    </section>
  )
}
